# Phase 21: Deploy Render - Research

**Researched:** 2026-10-12
**Domain:** Offline, deterministic, byte-faithful rendering of a declared Ignition 8.3 config tree (`gateways/<name>/` + `targets/<env>.yaml` + `modes/<env>/`) into the exact on-disk layout of a gateway deployment-mode collection; strict YAML schemas; secret/Windows/lockout refusals; first `ign deploy` verb.
**Confidence:** MEDIUM-HIGH. HIGH on the Rust design (prototyped and run this session on the real 183-file corpus), on codebase integration seams (read in source), and on the uuid/secret/type rule data derivable offline. MEDIUM-LOW on gateway-written tag-file bytes, tag path mapping, hostile-name encoding and Windows behaviour; those are capture-gated and listed in "Capture-first status".

<user_constraints>
## User Constraints (no CONTEXT.md exists for this phase)

The user chose to continue without `/gsd-discuss-phase`. Per the orchestrator, the ROADMAP Phase 21 section is treated as locked. Copied verbatim from `.planning/ROADMAP.md` (Phase 21):

### Locked Decisions (ROADMAP "Constraints" + "Success Criteria")

**Constraints**: Spec correction: preserve source key order. Spec §4 "sorted keys" is wrong; use an order-preserving node tree, Gson HTML-safe escaping, 2-space indent and no trailing newline, and rewrite a gateway file only when `write(parse(bytes)) == bytes`, otherwise copy it opaque. YAML via `yaml_serde` 0.10 (compiles on the 1.88 floor; not serde-saphyr); no `uuid` crate. The manifest and target schemas include `verify.tag_probes[]` (SF-8) and the target `ssh` block (SF-10) now, so strict parsing does not reject them in Phase 22. Overlays are add/override only; "absent in env" is documented, not built (SF-7). The render golden runs on `windows-latest`, and fixture `gateways/` trees carry no `\r`. `render` reaches the MCP catalog by clap derivation (Decision D).

**Success criteria** (what must be TRUE):
1. `ign deploy render` on the same tree and environment yields the same `render_sha256` on every run; a `manifest.yaml` or `targets/<env>.yaml` with an unknown field or a wrong `schema_version` fails as `deploy_tree_invalid` naming file, line and column
2. A file under `modes/<env>/` replaces its base-tree counterpart whole-file by path; only manifest-allowlisted resource types are rendered, and a tree that declares security-properties, security-levels, an API token or a user source is refused
3. Gateway-written files pass through byte-for-byte: the 183-file gateway corpus round-trips byte-identical on Linux, macOS and Windows. Files `ign` synthesizes come from a Gson-compatible writer that keeps source key order. A new named resource gets a deterministic name-based `attributes.uuid`, a singleton type gets none, and an existing uuid is kept
4. Render refuses an `Embedded` secret or a plaintext secret field as `embedded_secret`, naming the path and never the value, and refuses case-fold collisions, Windows-illegal names, paths over the Windows length budget and CR bytes anywhere in the tree
5. `deploy render` lands with its TUI route row, README entry and exit-table rows, its new slugs pass the four-place rule, and a family coverage test fails if any `deploy` leaf ships without them; every later verb phase keeps this test green

**Decisions in force (ROADMAP v1.6 header):** A (verdict envelope; not used by render), D (MCP exposure: `render`, `plan`, `pull`, `verify` ARE MCP tools via clap derivation; not in `GUARDED_OPS`), spec correction (source key order), Windows-safety is a render-time refusal.

### Claude's Discretion
Everything not stated above: crate-internal module layout, CLI flag names, the type-rule table representation, the uuid derivation function, lint finding format, `render_sha256` construction, fixture sanitization approach, and how the capture-first items are sequenced. Where this research makes a call that the user may want to veto, it is in the Assumptions Log.

### Deferred Ideas (OUT OF SCOPE)
- "Absent in env" / tombstone overlays (SF-7: documented, not built); templated or JSON-patch overlays (REQUIREMENTS Out of Scope)
- `plan`, `pull`, `verify`, transport, `apply`, `rollback`, `projects`, `--detailed-exitcode`, `similar`-based `plan --diff` (Phases 22-25)
- Guarded `--strip-core` (SF-5); secret values in the rendered tree (never)
- A real TUI deploy panel (registry rows only, workspace precedent)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DRND-01 | `ign deploy render` on a `gateways/<name>/` tree + env yields a rendered tree with identical `render_sha256` for identical inputs | `RenderedTree` over `BTreeMap<RelPath, Vec<u8>>`; digest construction in "Pattern 5"; no `HashMap`, no `read_dir` order, no `now()`; `--out` optional so a dry run returns the digest only |
| DRND-02 | `manifest.yaml` / `targets/<env>.yaml` parse strictly; unknown field or wrong `schema_version` fails with file, line, column | `yaml_serde` 0.10.7 `deny_unknown_fields`; line/column behaviour VERIFIED by running it (Pattern 2); schema_version-first two-pass; duplicate/missing-field location quirk documented |
| DRND-03 | `modes/<env>/` overrides base tree by path, whole-file | Pattern 3 (overlay on collection-relative paths); add/override only |
| DRND-04 | Only manifest-allowlisted types rendered; security singletons refused | Allowlist + hard deny table (`ignition/security-properties`, `ignition/security-levels`, `ignition/api-token`, `ignition/user-source`); Open Question OQ-A on refuse-vs-skip for un-allowlisted dirs |
| DRND-05 | Gateway-written files byte-for-byte; synthesized files via Gson-compatible order-preserving writer; corpus round-trips on Linux/macOS/Windows | RawValue-driven `Node` tree PROTOTYPED here: 178/183 corpus files byte-identical, the 5 others are non-Gson writers (opaque path). Corpus must be sanitized before commit (public repo) and marked `-text` |
| DRND-06 | New named resources get deterministic name-based uuid from a per-type rule table; singletons none; existing kept | Offline census of 47 types (corpus) + OpenAPI route taxonomy (87 types); three rule classes (singleton / named+uuid / named scope-G no uuid); `sha1` is NOT in the dependency graph; recommendation in Pattern 4 |
| DRND-07 | Refuse an Embedded secret or plaintext secret field, naming path never value | OQ8 largely answered offline: OpenAPI 8.3.6 gives the exact Embedded and Referenced JSON shapes and the 10 resource types with secret-typed fields; generic `"type":"Embedded"` catches third-party module types the OpenAPI does not describe |
| DRND-08 | Refuse case-fold collisions, Windows-illegal names, over-budget paths, CR bytes | Lint design in Pattern 6; CRITICAL: "CR bytes anywhere" literally rejects every JDBC driver jar (4/4 corpus jars contain thousands of 0x0D) - scope to text members (OQ-B) |
| DSURF-01 | Each deploy verb lands with TUI row, README entry, exit-table rows; four-place rule; family coverage test | Existing seams mapped (tui_coverage.rs, routes.rs workspace precedent, README exit table + `readme_exit_table_agreement`, `exit_code_mapping_enumerated`); new data-driven `deploy` family coverage test design in "Surface Parity" |
</phase_requirements>

## Summary

Phase 21 is a pure, offline library feature plus one thin CLI verb. The three hard parts are (1) byte fidelity of the rendered files, (2) the per-type rules (uuid, scope, secret fields, deny list) that must come from evidence, and (3) fail-closed lints that protect Windows production hosts. The design is settled and prototyped: an order-preserving `Node` tree built on `serde_json`'s already-enabled `raw_value` feature (zero new dependencies, number lexemes kept verbatim) reproduces 178 of the 183 corpus files byte-for-byte, and the remaining 5 are files written by other writers (two Cirrus Link compact files, `quickstart` compact, two with a trailing newline), which the roadmap's "rewrite only when `write(parse(bytes)) == bytes`, else copy opaque" rule handles by construction. YAML strictness with line and column works in `yaml_serde` 0.10.7 (run this session), with one caveat: missing-field and duplicate-field errors report line 1 column 1, so only unknown-field, wrong-type, wrong-`schema_version` and syntax errors can promise a real position.

Four findings change the plan. (a) `sha1` is not in the workspace dependency graph (checked all targets), so UUIDv5 needs a new crate; the zero-dependency route is a name-based UUID derived from the existing `sha2` and labelled version 8. (b) The dev-snapshot "183-file corpus" cannot be committed as-is: `ignition-cli` is a PUBLIC repo and the corpus holds 10 files with real `Embedded` ciphertext, an Entra tenant GUID and an internal IP; it needs a documented sanitization step and `-text` in `.gitattributes` (git for Windows would otherwise CRLF the fixtures and fail the Windows leg). (c) DRND-08's "CR bytes anywhere in the tree" must exclude binary members or every `database-driver` resource (a `.jar`) is refused. (d) Of the six capture-first items, OQ8 and the bulk of C4 are answerable offline from existing artifacts, C3b is half-answered by the spike, and C5, OQ1, OQ4 (plus the tag-type slice of C4) need one live capture task on the ephemeral 8.3.6 rig that Phase 20 already built (`crates/ignition-core/tests/fixtures/live-rig-deploy/`, bind mount proven viable, D11).

**Primary recommendation:** Plan Phase 21 as four waves: (0) one capture/fixture plan (sanitized corpus + `21-LIVE-CAPTURES.md` + the two new slugs under the four-place rule in one early commit), (1) `Node`/Gson writer + corpus gate and the strict YAML schemas, (2) layout/overlay/allowlist/type-rule table/uuid and the three lints, (3) the `deploy render` verb with TUI row, README, exit-table rows and the data-driven family coverage test. Do not write verb code until the capture doc exists.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Parse `manifest.yaml` / `targets/<env>.yaml` strictly | Core library (`ignition-core::deploy`) | CLI (maps error to envelope) | Pure data; core never prints; CLI and TUI share it |
| Order-preserving JSON `Node` + Gson writer + round-trip gate | Core library | - | Pure bytes-in/bytes-out; the corpus gate lives in core tests |
| Overlay, allowlist, layout (repo path to collection path) | Core library | - | One injective mapping shared by later `plan`/`apply`; must not live in the CLI |
| uuid minting, type-rule table, secret-field table | Core library | - | Deterministic, no I/O; table is data plus a test against a committed OpenAPI extract |
| Lints (secret, case-fold, Windows names, length, CR) | Core library | - | Fail-closed; return all findings at once |
| Reading the repo tree, writing `--out` | Core action (`actions/deploy.rs`) over a small `TreeSource` seam | CLI | Filesystem edge; symlink/clobber refusals live here |
| clap surface, `--json` envelope, exit mapping | CLI (`ignition-cli`) | - | `Commands::Deploy`, `ActionOutput::DeployRender`, `profile: null` |
| TUI route row, README entry, exit-table rows | TUI registry (`ignition-tui/routes.rs`) + README | - | Registry-only row on `Screen::Projects` (workspace precedent) |
| MCP exposure | CLI (`mcp.rs` clap derivation) | - | Automatic; no `GUARDED_OPS` entry for `render` (Decision D) |
| Gateway reads/writes | NOT this phase | - | Render is offline; no profile, no `Session`, no network |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `yaml_serde` | 0.10.7 (crates.io, published 2026-08-18; declared MSRV 1.82) | Strict typed YAML for `manifest.yaml` and `targets/*.yaml` | Roadmap-locked. The YAML org's maintained serde_yaml continuation (repo `yaml/yaml-serde`, owner github.com/ingydotnet, 3.1M downloads). Adds exactly one new lock entry (`libyaml-rs` 0.3.0, declared MSRV 1.60; `indexmap`, `itoa`, `ryu`, `hashbrown` already in `Cargo.lock`) [VERIFIED: crates.io API + `grep Cargo.lock` this session] |
| `serde_json` (existing, `raw_value` already enabled) | locked in workspace | `Node` parser via `Box<RawValue>` members | Zero new dependency; preserves key order and number lexemes without `preserve_order`/`arbitrary_precision` (which stay OFF; verified neither feature is unified on today) [VERIFIED: `cargo tree -e features -i serde_json`, workspace `Cargo.toml:serde_json`] |
| `sha2` (existing) | 0.10.9 | Per-file hash, `render_sha256`, name-based uuid | Already a production dep of `ignition-core` (`Cargo.toml`) [VERIFIED: `cargo tree -i sha2`] |
| `serde` (existing) | workspace | Derive for schemas and the `deploy render` result model | Existing |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `proptest` | 1.11.0 (existing dev-dep of ignition-core) | Writer idempotence `write(parse(write(x))) == write(x)`, hostile-name lint properties | Pattern from `tests/workspace_path_mapping.rs` |
| `tempfile` | 3.27 (existing) | Fixture trees and `--out` tests | Already a core dep |
| `snapbox` / `assert_cmd` / `predicates` | existing dev-deps | CLI contract and golden tests | `crates/ignition-cli/tests/contract_*.rs` pattern |
| `getrandom` 0.2 (existing) | - | NOT used by render (render must be pure) | Listed only to say: no randomness in render |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| sha2-derived version-8 name-based UUID (recommended) | `sha1` 0.10.7 + true UUIDv5 | `sha1` is absent from the graph (checked `--target all`); adding it costs one crate (RustCrypto, 578M downloads) and gives interoperable `uuid5(ns, name)` values any Python/Java tool can recompute. Choose it only if someone needs to recompute uuids outside `ign` |
| `RawValue`-driven `Node` (recommended) | Hand-written JSON lexer | The RawValue route reuses serde_json's validated lexer and needs ~60 lines; a hand lexer is a second JSON dialect to test |
| `RawValue`-driven `Node` | `serde_json` + `preserve_order` | Forbidden: workspace feature unification would reorder every `json!` payload in the frozen contract (ROADMAP Phase 20 constraint) |
| `yaml_serde` | `serde-saphyr` | Rejected by roadmap: MSRV 1.89 > 1.88 floor |

**Installation:**
```bash
# workspace Cargo.toml [workspace.dependencies]
yaml_serde = "0.10"
# ignition-core Cargo.toml [dependencies]
yaml_serde = { workspace = true }
```

**Version verification:** `yaml_serde` max stable 0.10.7 (2026-08-18), `rust_version` 1.82, repository `https://github.com/yaml/yaml-serde`; `libyaml-rs` 0.3.0, rust_version 1.60, repository `https://github.com/yaml/libyaml-rs` [VERIFIED: crates.io API, this session]. I compiled and ran `yaml_serde` 0.10.7 on rustc 1.94.1 only (no 1.88 toolchain installed); the 1.88 compile claim is the milestone research's (`research/SUMMARY.md` Conflict 1) plus the declared MSRVs [CITED: .planning/research/SUMMARY.md]. The planner should make "first `cargo build` on the 1.88 floor" part of the dependency-add task only if CI's `stable` toolchain is not considered sufficient (CI uses `toolchain: stable`, `.github/workflows/ci.yml`).

## Package Legitimacy Audit

> The `gsd-tools query package-legitimacy check` seam is not present in this install ("Unknown command: package-legitimacy"), so the audit below is manual against the crates.io API. Package names come from the roadmap (a project document) and Context7 resolved `yaml_serde` to `/yaml/yaml-serde` (High reputation), which is the authoritative-source confirmation.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `yaml_serde` 0.10.7 | crates.io | 8.5 months (created 2026-01-24) | 3.16M total | github.com/yaml/yaml-serde (YAML org; owner ingydotnet) | OK (young but org-owned, widely adopted; roadmap-locked) | Approved |
| `libyaml-rs` 0.3.0 (transitive of yaml_serde) | crates.io | 7 months | 3.08M total | github.com/yaml/libyaml-rs | OK | Approved (transitive) |
| `sha1` 0.10.7 / 0.11.0 (OPTIONAL alternative only) | crates.io | years | 578M | github.com/RustCrypto/hashes | OK | Not recommended (see uuid decision); planner adds `checkpoint:human-verify` only if chosen |

**Packages removed due to [SLOP]:** none. **Flagged [SUS]:** none. Crates.io packages have no npm-style postinstall; `libyaml-rs` ships no `build.rs` network step that I inspected (not opened this session) - low risk, but the dependency-add task should run `cargo build` and `cargo tree -p ignition-core | grep -c libyaml` to confirm exactly one new crate [ASSUMED: no build.rs side effects].

## Capture-first status (answers "already answered / offline / live capture needed")

| ID | Verdict | Evidence and what is still needed |
|----|---------|-----------------------------------|
| **OQ8** Embedded vs Referenced shape | **Answered offline** (key order of `Referenced.data` still a 1-minute live check) | Live 8.3.6 OpenAPI (`~/.cache/ign-deploy-rig/openapi-8.3.6.json`, also `whk-environment-orchestration/docs/openapi.json`) defines `{"type":"Embedded","data":{protected,encrypted_key,iv,ciphertext,tag}}` and `{"type":"Referenced","data":{providerName,secretName}}` with the oneOf `[null, Embedded, Referenced]`; the corpus shows the gateway-written Embedded form: `"password": {"data": {"ciphertext": …, "encrypted_key": …, "iv": …, "protected": …, "tag": …}, "type": "Embedded"}` (`ignition/database-connection/WHK01_PG_MES/config.json`, `data` keys alphabetical, `type` after `data`). 10 resource types carry secret-typed fields (table in Pattern 7). Still needed: create one Referenced secret via the rig to capture the gateway's own key order for `Referenced.data` [VERIFIED: openapi + corpus read this session] |
| **C4** named vs singleton census | **Mostly offline; tag types need live** | Corpus census over all 91 `resource.json` (47 types) gives a clean 3-way rule (Pattern 4); the OpenAPI route taxonomy (`/resources/<m>/<t>/{name}/{signature}` = named, else singleton) classifies 87 types (49 named, 38 singleton-ish). Not covered by either: `oauth2-client`, `alarm-notification-profile`, `roster-config`, `holiday`, `service-connector`, `tag-provider`, `tag-group`, `tag-type-definition`, `tag-definition`. Live task: create one of each on the rig (`ign api call POST /data/api/v1/resources/...`, `ign tags provider create`) and read the gateway-written `resource.json` from the bind mount. The prod `core` bootstrap named in the roadmap is not in the repo or reachable (and `ign` must not be pointed at WHK prod) [VERIFIED: computed from corpus + openapi this session] |
| **C3b** stale/omitted `lastModificationSignature` | **Half answered; one live probe** | Omitted case: hand-written `resource.json` with `uuid` and NO `lastModification`/`lastModificationSignature` loaded on 8.3.6 (`spikes/gateway-modes/s4_scale.sh:6` provider; `results/S2.md` tag files loaded; `fixtures/*/resource.json` carry only `{"enabled":true}`) [CITED: whk-environment-orchestration spikes]. Still open: (a) stale signature present, (b) does the gateway rewrite a hand-written `resource.json` after load, (c) is a flag/log emitted. Live probe: bind-mounted rig, write variants under `resources/production/…`, scan, `find`, re-read bytes, `ign logs` |
| **C5** hostile-name encoding | **Live capture required** (Linux rig sees gateway logic only, not NTFS) | Create tags/folders named with `:`, `?`, trailing `.`, spaces, commas, `Case`/`case` via `ign tags import` (JSON) or the config API into a provider, then list the bind-mounted `tag-definition/<P>/` directory. Tells whether the gateway percent-encodes. Windows filesystem behaviour stays INFERRED until captured on a Windows host (Phase 22's C7 work or a human step); render must stay fail-closed meanwhile. The "637 WHK01 tag files" are on prod only |
| **OQ1** gateway-written bytes of `tags.json`, `udts.json`, `unary-resource.json` | **Live capture required** | No gateway-written tag files exist anywhere on this machine (`mdfind` for `unary-resource.json`/`udts.json` empty; spike fixtures are hand-written compact one-liners). Create a provider, folders, atomic tags (include floats, a non-ASCII name, a large int, `<`/`&`/`=`/`'` in an expression), a UDT instance and a UDT type via `ign tags import` JSON; copy the resulting files out of the bind mount into the corpus. The existing corpus has ZERO floats and ZERO non-ASCII bytes (verified), so it proves neither number nor UTF-8 behaviour |
| **OQ4** repo-path to collection-path mapping | **Offline design + one live confirmation** | Spike scripts show the on-disk layout: `core/ignition/tag-provider/<P>/` and `core/ignition/tag-definition/<P>/<folder…>/{tags.json,unary-resource.json}` (`s2_tag_shadowing.sh:8-10`, `s4_scale.sh:7-9`). Spec §4's repo layout `tags/<provider>/{tag-provider,tag-group,tag-type-definition,tag-definition}/…` therefore maps to `ignition/<type>/<provider>/…` (type and provider swap). `tag-group` and `tag-type-definition` on-disk locations are by analogy [ASSUMED]; confirm in the same live capture |

**Rig and commands for the live task** (single ephemeral 8.3.6, localhost only, already built in Phase 20):
```bash
cd crates/ignition-core/tests/fixtures/live-rig-deploy
export IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/resources   # OUTSIDE the repo
docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml up -d
./mint-token.sh                                  # tokens.env, 0600, outside the repo
# reuse capture.sh's cap()/h()/assert_local; add subcommands c3b | c4 | c5 | oq1 | oq4, and run with
CAPTURE_OUT=.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md ./capture.sh <sub>
# tag/provider creation (no WebDev routes needed):
ign tags provider create CAP21 --profile <rig-profile>     # then ign tags import … --json
docker compose -p ign-deploy-rig down -v         # always tear down
```
Rules inherited from Phase 20: `assert_local` refuses non-localhost, tokens never printed, `ign` on PATH is the INSTALLED binary (`cargo install --path crates/ignition-cli --force` before any smoke of new verbs), never create a git worktree for this (shared `CARGO_TARGET_DIR`).

## Architecture Patterns

### System Architecture Diagram

```
 repo/                                  CLI: ign deploy render --env E [--repo DIR] [--out DIR]
  targets/E.yaml ──┐                          │ (profile: null, no Session, no network)
  gateways/G/      │                          ▼
   manifest.yaml ──┤        ┌─ actions::deploy::deploy_render(repo, env, out_opt) ──────────────┐
   resources/…     │        │ 1 read targets/E.yaml ─► strict parse (schema_version first)      │
   tags/…          │        │      └► Target{gateway:G, mode, nodes, ssh{…}}  ──(error: file:line:col)
   modes/E/…       │        │ 2 read gateways/G/manifest.yaml ─► strict parse ─► Manifest        │
                   │        │ 3 TreeSource.read_all(): BTreeMap<RelPath, bytes>                  │
                   │        │      refuse symlinks, non-UTF-8 names, dot-names, size caps        │
                   ▼        │ 4 overlay: base(resources/*,tags/*) ⊕ modes/E/* by path (whole file)│
        ┌──────────────┐    │ 5 layout: repo path ─► collection-relative path  (injective)       │
        │ RULE DATA    │───►│ 6 allowlist + deny (security singletons) ─► refuse                 │
        │ type rules   │    │ 7 per JSON file: parse to Node; gate write(parse(b))==b            │
        │ secret fields│    │      gateway-format ► keep bytes; non-canonical ► opaque verbatim  │
        │ win name set │    │ 8 uuid: named+uuid type, resource.json lacks uuid ► synthesize     │
        └──────────────┘    │      (new resource.json via Gson writer, source key order)         │
                            │ 9 LINTS (collect ALL findings, no early exit):                     │
                            │      embedded/plaintext secret ► embedded_secret (paths only)      │
                            │      case-fold, Windows names, length, CR(text only) ► deploy_tree_invalid
                            │10 RenderedTree{files: BTreeMap, render_sha256}                      │
                            │11 --out: write bytes (clobber ladder) | else digest-only           │
                            └──────────────┬─────────────────────────────────────────────────────┘
                                           ▼
        envelope {ok:true, profile:null, data:{env,gateway,mode,out,files,bytes,render_sha256,
                  resources[],tag_providers[],uuids_minted,opaque_files,warnings[]}}  exit 0
        failures: CoreError::DeployTreeInvalid (exit 2) | CoreError::EmbeddedSecret (exit 2)
```

### Recommended Project Structure
```
crates/ignition-core/src/deploy/
├── mod.rs          # module map + doc; `pub mod deploy;` in lib.rs next to rig/module/webdev
├── manifest.rs     # Manifest, ProjectPin, Verify{tag_probes,timeouts}; deny_unknown_fields
├── target.rs       # Target, Node{profile}, Ssh{host,port,user,key_file|key_env,os,data_dir}
├── yamlio.rs       # strict_parse<T>(file, text) -> CoreError::DeployTreeInvalid{file,line,column}
├── node.rs         # Node tree, RawValue parser, Gson writer, canonical-or-opaque gate
├── layout.rs       # RelPath newtype ('/'-only), repo<->collection mapping, ResourceKey
├── rules.rs        # TypeRule table (scope, uuid kind), DENY list, SECRET_FIELDS table
├── uuid.rs         # name-based uuid from sha2
├── lint.rs         # secret / case-fold / windows-name / length / CR findings
├── tree.rs         # TreeSource (fs + in-memory), RenderedTree, render_sha256, write_out
└── render.rs       # render(source, manifest, target) -> RenderedTree + report
crates/ignition-core/src/actions/deploy.rs         # deploy_render (+ result model)
crates/ignition-cli/src/{cli.rs,main.rs,render.rs} # Deploy(DeployArgs){Render}, ActionOutput::DeployRender
crates/ignition-tui/src/routes.rs                  # `deploy render` row -> Screen::Projects
crates/ignition-core/tests/fixtures/deploy/        # sanitized corpus, tag captures, openapi extract
```
This matches the milestone ARCHITECTURE §3.1 layout, with `gson.rs` named `node.rs` because the Node tree, not only the writer, is the contract.

### Pattern 1: Order-preserving `Node` from `RawValue` (VERIFIED by prototype)
**What:** Parse each object's members as `Vec<(String, Box<RawValue>)>` via a `visit_map`, recurse on the raw text; keep number/true/false/null as their original lexeme; write with 2-space indent, `": "`, no trailing newline, `{}`/`[]` for empties, Gson HTML-safe escapes (`< > & = '` as `\u003c \u003e \u0026 \u003d \u0027`), `\u2028/\u2029` escaped, other non-ASCII raw.
**When to use:** every JSON file. Gate: `if write(parse(bytes)) == bytes` the file is "gateway-format" (rewritable, e.g. for uuid injection); otherwise it is copied opaque and may not be mutated (a mutation request on an opaque file is `deploy_tree_invalid`).
**Evidence:** I built this in a scratch crate (workspace `serde_json` features `raw_value` only) and ran it on `gw-config/`: `identical=178 not_identical=5`. A separate Python implementation also gave 178/5, same five files. The five: `config-mode.json` and `ignition/security-properties/config.json` (trailing newline), `ignition/quickstart/config.json` (compact one-liner) and the two Cirrus Link `namespace-server-set/*/config.json` (compact) [VERIFIED: executed this session against the real corpus].
```rust
// Source: scratch prototype executed this session; serde_json raw_value (docs.rs/serde_json/latest/serde_json/value/struct.RawValue.html)
enum Node { Obj(Vec<(String, Node)>), Arr(Vec<Node>), Str(String), Lit(String) /* number|true|false|null lexeme */ }
fn parse(raw: &str) -> Result<Node, serde_json::Error> {
    let t = raw.trim_start();
    match t.as_bytes().first() {
        Some(b'{') => { let m: Members = serde_json::from_str(t)?; /* Members::visit_map => next_entry::<String, Box<RawValue>>() */
                        Ok(Node::Obj(m.0.into_iter().map(|(k, v)| parse(v.get()).map(|n| (k, n))).collect::<Result<_, _>>()?)) }
        Some(b'[') => { let a: Vec<Box<RawValue>> = serde_json::from_str(t)?; Ok(Node::Arr(a.into_iter().map(|v| parse(v.get())).collect::<Result<_, _>>()?)) }
        Some(b'"') => Ok(Node::Str(serde_json::from_str(t)?)),
        _ => Ok(Node::Lit(t.trim_end().to_string())),
    }
}
```
Caveats to plan for: (1) duplicate keys are preserved, not detected, so the lint must reject them explicitly; (2) each level re-parses its substring, so cost is O(depth x size), fine for config files but cap file size (suggest 16 MiB per JSON file); (3) a UTF-8 BOM or invalid UTF-8 makes a file opaque, not an error; (4) numbers are never reformatted, so the gate, not the writer, decides whether a hand-edited number form is acceptable.

### Pattern 2: Strict YAML with real positions (VERIFIED by running yaml_serde 0.10.7)
Run on rustc 1.94.1 with `deny_unknown_fields` structs:

| Input defect | Result | Position usable? |
|--------------|--------|------------------|
| unknown top-level key `bogus` | `unknown field \`bogus\`, expected one of …` | yes: `line 3 column 1` |
| unknown nested key under `verify` | `verify: unknown field \`extra\`, expected \`tag_probes\`` | yes: `line 5 column 3` |
| wrong type (`schema_version: one`) | `invalid type: string "one", expected u32` | yes: `line 1 column 17` (note: echoes the value) |
| wrong `schema_version` via a `SchemaVersion` newtype whose `Deserialize` raises `custom` | `unsupported schema_version 2; this ign reads 1 at line 2 column 1` | yes (column is the key start) |
| YAML syntax error / tab indent | `found character that cannot start any token at line 3 column 1` | yes |
| missing required field | `missing field \`schema_version\`` | NO: reports (1,1) |
| duplicate key | `duplicate field \`schema_version\`` | NO: reports (1,1), not the duplicate's line |
| anchor/alias `&x` / `*x` | accepted and expanded silently | n/a |

Consequences for the plan: format the error ourselves as `{file}:{line}:{column}: {message}` using `e.location()`, stripping yaml_serde's embedded ` at line N column M` suffix to avoid a double position; do the schema check FIRST with a probe struct `{ schema_version: SchemaVersion }` that has no `deny_unknown_fields` (otherwise an unknown key earlier in the file masks the version error), then the strict typed parse; tests may only assert exact line/column for unknown-field, wrong-type, wrong-version and syntax errors, and must assert only "names the file" for missing/duplicate fields. The "no anchors" idea came from `research/SUMMARY.md`, not the roadmap; the parser accepts them. Recommend a 1 MiB size cap on YAML inputs instead of a brittle anchor scan (Open Question OQ-E).

### Pattern 3: Overlay and layout on ONE injective path type
`RelPath` = `/`-separated, UTF-8, no `.`/`..`/empty components, no leading `/`, no `\`, built only through a validating constructor (the `workspace` path-mapping lesson: single source of truth, fail closed). Order of operations: read base files (`resources/<module>/<type>/<name>/…` and `tags/<provider>/<type>/…`), read `modes/<env>/**` with the SAME relative structure, overlay by identical repo-relative path (overlay wins whole-file, adds when no base counterpart), THEN map to collection-relative paths (`resources/` stripped; `tags/<P>/<type>/<rest>` to `ignition/<type>/<P>/<rest>`, OQ4). Run all lints on the merged, mapped tree so collisions between an overlay and a base path under different spellings are caught. `config-mode.json` anywhere in the tree is gateway-owned (it is what `POST /mode` writes, `parent: core`): refuse it as `deploy_tree_invalid` (OQ-D). Never read or write `modes/<env>/` of OTHER environments into the render.

### Pattern 4: Per-type rule table (uuid, scope) from evidence
Offline census over the corpus (`gw-config/`, 91 `resource.json`), computed this session:
- **Singleton** (resource directly under `<module>/<type>/`, no name segment, no uuid): `ignition/{cobranding, edge-sync-settings, edge-system-properties, gateway-network-proxy-rules, gateway-network-queue-settings, gateway-network-settings, general-alarm-settings, quickstart, security-levels, security-properties, system-properties}`, `com.inductiveautomation.eam/{event-thresholds, module-settings}`, `…opcua/{access-control, one-time, server-config}`, `…sfc/chart-settings`, and the Cirrus Link `general` / `namespace-file` singletons. OpenAPI route taxonomy independently lists these as non-named.
- **Named, with uuid, `scope: "A"`:** all other types seen: `database-connection`(5/5 uuid), `database-driver`(6/6), `database-translator`(6/6), `identity-provider`, `opc-connection`, `email-profile`, `alarm-journal`, `audit-profile`, `schedule`, `security-zone`, `user-source`, `device`, `historian-provider`, `agent-group`, `module-certificates`, `module-eulas`, the Cirrus Link named types.
- **Named, NO uuid, `scope: "G"`:** exactly two types: `ignition/store-and-forward-engine` (5 resources, uuid 0/5) and `com.inductiveautomation.perspective/themes` (4, uuid 0/4). `scope` is `G` for these 9 resources and `A` for the other 82. This is the trap behind "30 of 91 have no uuid": it is not only singletons. Store-and-forward is in the spec §4 phase-1 declared list, so a naive "named => mint uuid" rule would add a uuid the type never carries. [VERIFIED: computed from corpus this session; what `G`/`A` mean is [ASSUMED]]
- Other `resource.json` facts: key order is `scope, [description,] version, restricted, overridable, files, attributes` (63 resources without `description`, 28 with); `attributes` order `lastModification, [legacyId,] uuid, lastModificationSignature, enabled` with members optional; `version` is `1` everywhere; `files` lists every member file including non-JSON (`index.css`, `*.jar`, `users.json`).

Table shape (one row per allowlist-eligible type; every manifest-allowlisted type MUST have a row or manifest load fails as `deploy_tree_invalid`, fail-closed):
```rust
enum UuidRule { Singleton /* none */, Named /* mint if absent */, NamedNoUuid /* scope G */ }
struct TypeRule { module: &'static str, ty: &'static str, uuid: UuidRule, scope: char }
```
Test: the table equals a committed extract (`tests/fixtures/deploy/openapi-8.3.6-resource-types.json`, generated by a documented script from the live OpenAPI, the way `20-openapi-extract.json` was) for named-vs-singleton, and a corpus test asserts every corpus `resource.json` agrees with its row. Types missing from corpus and OpenAPI (`oauth2-client`, `alarm-notification-profile`, `roster-config`, `holiday`, `service-connector`, tag types) get rows only after the live C4 capture.

**uuid derivation (recommendation, Assumptions A1):** `uuid = hex(sha256("ign-deploy:uuid:v1\0" + gateway + "\0" + module + "/" + type + "/" + name)[..16])` with the version nibble forced to `8` and the variant bits to `10` (RFC 9562 version 8, the sanctioned slot for vendor-specific hash-derived ids), rendered lowercase `8-4-4-4-12`. Zero new dependency; env is deliberately excluded so a resource keeps one identity across `test`/`production` renders of the same gateway tree. An existing uuid in `resource.json` is always kept (never recomputed, never copied from another collection). It is only synthesized for `Named` types whose `resource.json` lacks one, or when `ign` synthesizes the whole `resource.json` for a new resource that has only `config.json`. Synthesized shape (the shape the spike gateway already loaded): `{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json",…],"attributes":{"uuid":"…","enabled":true}}` through the Gson writer (so pretty-printed, source order as listed). Live check required before freezing: the gateway loads a resource carrying a version-8 uuid (Assumptions A1) and does not complain about absent `lastModification`.

### Pattern 5: `render_sha256`
Bare lowercase hex SHA-256 over a canonical listing: for each file in byte-sorted `RelPath` order append `path\0<sha256 hex of content>\n`, hash the concatenation. Independent of OS, separators, timestamps and read order (`BTreeMap` iteration; never `read_dir` order). Include `render_sha256` and per-file sha256 in `--out/render.json`; keep the listing format documented in README because Phase 22's `plan` and `--base` consume it. Do NOT include `render.json` itself in the digest.

### Pattern 6: Lints return everything at once
All lints share one `Vec<Finding{path, kind}>` collector; the action raises one error with the first N (suggest 20, then "and M more") so an operator fixes a tree in one pass. Kinds and the rule for each:
- **Case-fold collision:** group siblings (same parent) by `str::to_lowercase()` (NTFS uses its own upcase table; `to_lowercase` is the conservative portable approximation [ASSUMED]); more than one distinct member refuses. Also refuse a file/dir name colliding with a sibling under NFC normalization only if cheap; not required.
- **Windows-illegal names:** any component containing `< > : " | ? *` or a control character 0x00-0x1F; ending in `.` or space; component length over 255 UTF-16 units; reserved device stem `CON PRN AUX NUL COM1-9 LPT1-9` (with or without an extension, case-insensitive). The Windows rules apply to EVERY environment because `resources/` is shared across envs.
- **Length budget:** `len(target.ssh.data_dir) + len(r"\config\resources\") + len(mode) + 1 + len(rel with backslashes)` must stay under 248 (PITFALLS Pitfall 15 "assert max rendered path length plus prefix < 248"); when the target has no `data_dir` use the default prefix `C:\Program Files\Inductive Automation\Ignition\data\config\resources\production`, which is 79 characters [VERIFIED: counted]. The corpus's longest relative path is 135 characters [VERIFIED: measured], so tag trees are the real risk. The 248 figure is [ASSUMED: INFERRED from NTFS MAX_PATH behaviour, not captured on WIGNVM01].
- **CR bytes:** see Common Pitfalls 3 (scope to text members).
- **Dot-named components:** Designer throws `NameInvalidException` on dot-named entries under `resources/` (ROADMAP Phase 22 constraint). Refuse any dot-named component; skip only `.gitkeep` (OQ-D).
- **Duplicate JSON keys, symlinks, non-UTF-8 path bytes, `..`:** refuse (`deploy_tree_invalid`).

### Pattern 7: Secret detection data (OQ8, VERIFIED from live 8.3.6 OpenAPI)
Secret-typed fields are those whose schema is `oneOf [null, Embedded Secret, Referenced Secret]`. Walking the live OpenAPI request bodies, resource types with such fields and their real JSON paths (the OpenAPI path includes `oneOf` segments that are not in the file, strip them):

| Resource type | Secret field paths (inside `config`) |
|---------------|--------------------------------------|
| `ignition/database-connection` | `password` |
| `ignition/identity-provider` | `settings.clientSecret` |
| `ignition/oauth2-client` | `clientSecret` |
| `ignition/email-profile` | `settings.password` |
| `ignition/opc-connection` | `settings.authentication.password`, `settings.authentication.privateKeyPem`, `settings.security.keyStoreAliasPassword` |
| `com.inductiveautomation.opcua/device` | `settings.advanced.password` |
| `com.inductiveautomation.alarm-notification/alarm-notification-profile` | `settings.settings.password`, `settings.twoWayPop3.pop3Password`, `settings.voipGateway.password` |
| `ignition/service-connector` | `settings.password`, `settings.ssl.keyPassword`, `settings.ssl.keystore.key`, `settings.ssl.keystore.password`, `settings.ssl.truststore.password` |
| `ignition/system-properties` | `errorReportPassword` |
| `ignition/user-source` | `settings.connectionPassword`, `settings.ldap.connectionPassword` (type is denied anyway) |

Corpus cross-check: the gateway-written files use exactly these paths (`password`, `settings.authentication.password`, `settings.security.keyStoreAliasPassword`, `settings.clientSecret`) [VERIFIED]. The rule: at a secret-typed path the value must be `null` or an object with `"type":"Referenced"`; an `Embedded` object OR any string/other value is `embedded_secret`. Independently, ANY object with `"type":"Embedded"` and an object `data` anywhere in a `config.json` is refused (this is what catches the Cirrus Link MQTT `server`/`user` types the OpenAPI does not describe; 4 of the 10 corpus files with `Embedded` are Cirrus Link). Plaintext secret fields in third-party module types cannot be schema-driven; treat that as a documented limitation and add a field-name backstop that WARNS (not refuses) on `*password*`/`*secret*` string values, since a refusing heuristic would false-positive on `primaryKeyDef`-style names. The error names JSON paths (`ignition/database-connection/WHK01_PG_MES/config.json: password`) and never a value.

### Anti-Patterns to Avoid
- **Sorting keys anywhere.** `config.json` is mostly but not always alphabetical; `resource.json` never is. Render preserves source order (spec §4 is wrong).
- **Re-serializing everything.** Only rewrite when the round trip is identity; otherwise bytes pass through untouched.
- **`serde_json::Value` / `Map` for gateway files** (sorted BTreeMap; reformats floats). And no `preserve_order`/`arbitrary_precision`.
- **`HashMap`, `read_dir` order, `SystemTime`, locale** anywhere in render.
- **Erroring on the first lint finding.** Collect all.
- **Putting `--out` writes behind no safety.** Render is an MCP tool (Decision D) and `--out` is an arbitrary directory write: refuse a non-empty `--out` unless it holds a previous `render.json` marker, never follow symlinks, never delete.
- **Reusing `MemberSource::Tree` or `workspace` path mapping** (project-export layout, FNV-1a hash; `ARCHITECTURE.md` §4.2).
- **Adding `plan`-era deps now** (`similar`, transport). Phase 21 needs only `yaml_serde`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| YAML parsing + unknown-field refusal + positions | a YAML parser or line scanner | `yaml_serde` `deny_unknown_fields` + `Error::location()` | Dialect risk (`yes`/`no`, anchors, tabs); positions come free |
| JSON lexing/validation | a custom JSON lexer | `serde_json` + `RawValue` | Already validated; ~60 lines on top |
| SHA-256 / hex | own hash | `sha2` 0.10.9 (existing) | In the tree already |
| SHA-1 / UUIDv5 | own SHA-1 | either the sha2-derived v8 id (recommended) or the `sha1` crate | A hand-rolled SHA-1 is exactly what not to write |
| Unicode lowercasing for case-fold | own table | `str::to_lowercase` | Conservative superset check; NTFS table is not reproducible portably |
| File-system walking determinism | `read_dir` order | `std::fs::read_dir` then explicit `sort` into a `BTreeMap` | Determinism is the DRND-01 contract |
| Exit-code / slug plumbing | new mechanism | `CoreError` + the four-place rule | Machine-checked by existing tests |

**Key insight:** The only genuinely custom code is the Gson-compatible writer and the lint/rule tables, and both are pinned by a real corpus and committed extracts. Everything that looks like infrastructure already exists in the tree.

## Runtime State Inventory

Not applicable: Phase 21 is greenfield code (a new module and one new verb). It renames and migrates nothing. Stored data, live service config, OS-registered state, and secrets/env vars: none, since render is offline and touches no gateway. Build artifacts: none stale. (Verified by scope: no existing `deploy` verb or module exists; `grep -r "deploy_tree_invalid\|embedded_secret" crates README.md` returns nothing.)

## Common Pitfalls

### Pitfall 1: The corpus is a public-repo leak waiting to happen
**What goes wrong:** Copying `gw-config/` (the "183-file corpus") into `crates/ignition-core/tests/fixtures/deploy/` commits 10 files of real `Embedded` ciphertext (KEK-bound, `iat` in the JWE header), an Entra tenant GUID (in the OIDC issuer URL), an internal plant-network address, plant database names and OPC device configs into `TheThoughtagen/ignition-cli`, which is **PUBLIC** (`gh repo view` this session).
**Why it happens:** The roadmap says "183-file corpus" without saying the corpus is real WHK data.
**How to avoid:** A sanitization step, documented in `fixtures/deploy/SANITIZE.md` and re-runnable: replace every `Embedded` `data` value with synthetic same-shape base64url strings of equal length, replace the tenant GUID, IPs and plant-specific hostnames/usernames with fixed fakes, leave structure, key order, escapes and bytes of everything else alone (the round-trip property survives because the substitutions are value-only). Exclude the four JDBC jars (1-2.5 MB each) and keep tiny stand-ins with the same names so `files` lists stay valid; keep the 8 CSS files. Add a test that fails if any fixture contains a `ciphertext` value not matching the synthetic marker. Do the sanitization and the secret scan in the first plan, before the fixtures are staged.
**Warning signs:** `git diff --stat` showing fixtures before the scan exists; any `"ciphertext":` value that is not the marker.

### Pitfall 2: Windows checkout rewrites the fixtures and the golden fails only on `windows-latest`
**What goes wrong:** Git for Windows converts LF to CRLF on checkout (the repo has no `* text=auto`, but git for Windows defaults `core.autocrlf=true`), so the byte-identity gate and the render golden fail on the Windows CI leg with no code defect.
**How to avoid:** `.gitattributes` in `ignition-cli` must carry `crates/ignition-core/tests/fixtures/deploy/** -text` (precedent: `crates/ignition-core/tests/fixtures/*.xml -text` already exists there) and fixture `gateways/` trees must contain no `\r` (roadmap). Add a test that reads the fixtures and asserts zero `\r` in text members, so the failure is a clear message instead of a hash mismatch. The CI matrix already runs `windows-latest` (`.github/workflows/ci.yml` `os: [ubuntu-latest, macos-latest, windows-latest]`), so "the render golden runs on windows-latest" needs no CI change, only that tests do not shell out or assume `/`.
**Warning signs:** golden mismatch only on Windows; hashes differ by exactly the line count.

### Pitfall 3: "CR bytes anywhere in the tree" rejects every JDBC driver
**What goes wrong:** DRND-08 / success criterion 4 read literally refuses any file containing byte `0x0D`. All four `database-driver` jars in the corpus contain thousands (postgresql-42.7.2.jar: 3,885 CR and 30,129 NUL bytes; mariadb 2,374; mysql 8,998; mssql 4,879) [VERIFIED: counted]. A literal lint makes `ignition/database-driver` unrenderable, and `database-driver` is in the declared list.
**How to avoid:** Apply the CR check to TEXT members only: files whose extension is in a text list (`.json .css .yaml .yml .txt .md .xml .py .csv .properties .html .js .svg`) OR that are valid UTF-8 with no NUL byte; everything else is a binary member, exempt, and copied byte-for-byte. Record the rule in README and make `.gitattributes` mark jars `binary`. Flagged as Open Question OQ-B because it narrows a stated requirement.
**Warning signs:** `deploy_tree_invalid` naming a `.jar`.

### Pitfall 4: Perpetual diffs from a writer that is "almost" Gson
**What goes wrong:** Any byte difference between `ign` output and gateway output makes later `plan` non-empty forever.
**How to avoid:** The identity gate plus the corpus gate. Remember the corpus has no floats and no non-ASCII, so the gate alone does not prove those; the live OQ1 capture must add tag files with floats (`1.5`, `1.0E10`-style), a large integer, a non-ASCII name and `<&='>` text, and hostile-data proptests must cover them. Numbers are kept as source lexemes so a hand-written `2.50` is gated opaque rather than silently reformatted.
**Warning signs:** non-empty `plan` right after `apply`; a corpus file moving from "identical" to "opaque".

### Pitfall 5: Minting a uuid for a type that never has one
**What goes wrong:** `store-and-forward-engine` and `themes` are named types with no uuid (scope `G`); minting adds a shape change. Conversely a singleton given a uuid is also a shape change, and a named `A`-scope resource without one is silently never loaded ("does not have required UUID attribute, will not be loaded", `spikes/results/RIG.md`).
**How to avoid:** The three-class rule table (Pattern 4), a missing table row failing closed, and a corpus test asserting every real `resource.json` agrees with its row. Existing uuids are never recomputed.

### Pitfall 6: YAML error positions are not uniformly available
**What goes wrong:** A test promising "file, line and column" for a missing or duplicate field fails because `yaml_serde` reports (1,1).
**How to avoid:** Pattern 2 table. Assert exact positions only where the library provides them; for the others assert the file name and message.

### Pitfall 7: The `--out` directory write is an MCP-reachable file primitive
**What goes wrong:** `deploy render --out /anything` over MCP (no `confirm`, since `render` is not guarded) can overwrite files if naively implemented.
**How to avoid:** `--out` is optional (default = digest-only dry run, writes nothing). When given: create if absent, accept if empty or if it contains only a previous render marker (`render.json`), otherwise refuse (`invalid_input`, exit 2, the `workspace checkout` "unrelated non-empty directory refuses" ladder in `actions/workspace.rs`); write regular files only; never follow or create symlinks; never delete. Alternatively register `deploy render` in `GUARDED_OPS` only when `--out` is set; the roadmap/DSURF-02 keep `render` unguarded, so prefer the clobber ladder.

### Pitfall 8: Four-place slug drift
**What goes wrong:** Adding the two slugs in `error.rs` but missing the header table, `EXIT_SLUG_LITERALS`, the enumerated test, or the README row fails `readme_exit_table_agreement` or `exit_code_mapping_enumerated`; a hint naming a nonexistent command is a documented anti-pattern (the `RoutesNotDeployed` rule in `error.rs`).
**How to avoid:** One commit for `error.rs` + README (Phase 20-02 precedent: subject `feat(21-NN): deploy_tree_invalid and embedded_secret slugs (four-place rule)`). The two variants: `DeployTreeInvalid` (exit 2) and `EmbeddedSecret` (exit 2). Box the payload if it grows past two Strings (`WINDOWS.md` entry 3: `CoreError` is size-bounded; `clippy::result_large_err`).

### Pitfall 9: `cargo` worktree/target-dir trap
Never create a git worktree for this phase (shared `CARGO_TARGET_DIR`, project `.claude/CLAUDE.md`). Run all cargo commands on the main checkout; run `cargo install --path crates/ignition-cli --force` before any manual `ign deploy render` smoke (PATH resolves the installed binary).

## Code Examples

### Strict parse wrapper (derived from the run in Pattern 2)
```rust
// Source: yaml_serde 0.10.7 behaviour verified by execution this session
#[derive(serde::Deserialize)]
struct SchemaProbe { schema_version: SchemaVersion }          // no deny_unknown_fields
struct SchemaVersion;
impl<'de> serde::Deserialize<'de> for SchemaVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match u32::deserialize(d)? { 1 => Ok(Self), v => Err(serde::de::Error::custom(format!("unsupported schema_version {v}; this ign reads 1"))) }
    }
}
fn strict<T: serde::de::DeserializeOwned>(file: &str, text: &str) -> Result<T, CoreError> {
    let wrap = |e: yaml_serde::Error| {
        let (l, c) = e.location().map(|m| (m.line(), m.column())).unzip();
        CoreError::deploy_tree_invalid(file, l, c, strip_position(&e.to_string()))
    };
    yaml_serde::from_str::<SchemaProbe>(text).map_err(wrap)?;   // version first
    yaml_serde::from_str::<T>(text).map_err(wrap)                // then #[serde(deny_unknown_fields)]
}
```

### Schema skeletons that must exist NOW (SF-8, SF-10)
```rust
// Source: research/SUMMARY.md SF-8/SF-10 + spec §5.4 (design; field names are this research's recommendation)
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Manifest { schema_version: SchemaVersion, standards_version: Option<String>,
                  types: Vec<String> /* "module/type" allowlist */, tag_providers: Vec<String>,
                  projects: Vec<ProjectPin> /* {name, version, sha256, artifact} (Phase 25 consumes) */,
                  verify: Option<Verify> }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Verify { tag_probes: Vec<String> /* tag paths */, timeouts: Option<VerifyTimeouts> }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Target { schema_version: SchemaVersion, gateway: String /* gateways/<gateway> */, mode: String,
                nodes: Nodes /* {master: Node{profile, ssh}, backup: Option<Node>} */,
                restart: Option<Restart> }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Ssh { host: String, port: Option<u16>, user: String, key_file: Option<String>, key_env: Option<String>,
             os: Os /* windows|linux */, data_dir: String }
```
Keep every Phase 22/23 field OPTIONAL here (strictness only rejects what is not yet declared); credentials are profile names and env var NAMES, never values.

### `--env` resolution
`--env E` selects `targets/E.yaml`; its `gateway` names `gateways/<gateway>/`; the overlay directory is `gateways/<gateway>/modes/E/` (spec §4: "`modes/E/` overlaid by path"). `--repo` defaults to `.`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Spec §4 "sorted keys" | Preserve source order, identity-gated rewrite | falsified by the corpus (0 of 91 `resource.json` sorted); confirmed again here | writer is ordered, not sorted |
| `serde_yaml` | `yaml_serde` (YAML org) | serde_yaml archived; `serde_yml` unsound (RUSTSEC-2025-0068); `serde-saphyr` MSRV 1.89 | roadmap-locked |
| UUIDv4 persisted in repo | Deterministic name-based id at render time | milestone Conflict 5 | render stays pure, `plan` stays empty |

**Deprecated/outdated:** `ARCHITECTURE.md` §3.1 `deploy/gson.rs` "sorted keys" and PITFALLS 14 "config.json sorted/escaped, resource.json fixed struct order" (per-file-kind serializers) are superseded by one order-preserving writer plus the identity gate. PITFALLS Pitfall 7's "UUIDv5 ... stored once in the declared tree" is superseded by render-time derivation per the roadmap.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The gateway loads a resource whose `attributes.uuid` is a version-8 (sha2-derived) UUID and accepts absent `lastModification`/`lastModificationSignature` | Pattern 4 | Synthesized resources never load; fall back to `sha1` + real UUIDv5 or a v4-shaped id. Verify in the live C3b/C4 capture before freezing the derivation |
| A2 | `scope: "G"` marks the no-uuid named types and `"A"` the uuid ones | Pattern 4 | A type outside the corpus has a different pairing; the rule table is keyed on type, not on scope, so impact is limited to mis-synthesized `resource.json` for new resources |
| A3 | Gson writes doubles via `Double.toString` and non-ASCII raw UTF-8; the corpus contains neither | Pitfall 4 | Floats/UTF-8 in tag files would be gated opaque (safe) rather than rewritten; only uuid injection into such a file would be blocked |
| A4 | Windows path budget 248 and NTFS rules (`< > : " | ? *`, trailing dot/space, reserved names, case-insensitivity) are as listed | Pattern 6 | Over- or under-refusal; INFERRED, not captured on a Windows gateway |
| A5 | `tag-group` and `tag-type-definition` live at `ignition/<type>/<provider>/…` by analogy with `tag-definition` | OQ4 | Layout mapping wrong for two tag types; confirmed in the live capture |
| A6 | The gateway does not rewrite a hand-written `resource.json` after load (second half of C3b) | Capture status | If it rewrites, a clean `render` -> `apply` -> `plan` would never be empty; affects Phase 22 compare rules |
| A7 | The gateway encodes (or does not encode) hostile characters in tag directory names | Capture status (C5) | Unknown until captured; render stays fail-closed |
| A8 | `yaml_serde` 0.10.7 compiles on rustc 1.88 (only 1.94.1 was run here) | Standard Stack | CI `stable` would still pass; a 1.88-only break would surface in the dependency-add task |
| A9 | Un-allowlisted type directories present in the tree should be REFUSED (not silently skipped) | OQ-A | If the user wants silent skip, a stray directory renders nothing instead of failing |
| A10 | `.gitkeep` is the only tolerated dot-name (skipped, never rendered); all other dot-names refuse | Pattern 6 | A tree with other dot files fails render until cleaned |
| A11 | The uuid name excludes the environment | Pattern 4 | If the user wants per-env identities, include `mode` in the name before any plan depends on it |
| A12 | `libyaml-rs` has no side-effecting `build.rs` | Package audit | Supply-chain; verify with `cargo build` + review of the crate's build script |

## Open Questions

1. **OQ-A: refuse or skip a directory whose type is not allowlisted?** Roadmap: "only manifest-allowlisted resource types are rendered". What we know: milestone ARCHITECTURE §7.1 lists "path outside the allowlist" under `deploy_tree_invalid`; SUMMARY says "unlisted types stay gateway-owned". What's unclear: whether a stray `resources/ignition/foo/` should fail or be ignored. Recommendation: refuse (fail-closed; silence hides intent and `pull` will otherwise produce un-rendered trees). Needs user confirmation (A9).
2. **OQ-B: scope of "CR bytes anywhere".** See Pitfall 3. Recommendation: text members only. Needs user confirmation because it narrows success criterion 4 as written.
3. **OQ-C: where do the YAML/`render.json` files and corpus fixtures live and who owns sanitization?** Recommendation: fixtures under `crates/ignition-core/tests/fixtures/deploy/`, sanitization script documented in-repo (Python via `uv run`, per the user's global Python rule: never `pip install` into system Python), run once by the executor, output committed.
4. **OQ-D: `config-mode.json` and dot-names in the tree.** Recommendation: refuse `config-mode.json` (gateway-owned, written by `POST /mode`) and any dot-named component except a skipped `.gitkeep`.
5. **OQ-E: anchors/aliases in YAML.** The parser accepts them. Recommendation: do not ban; cap YAML inputs at 1 MiB.
6. **OQ-F: does C5 need a Windows capture inside this phase?** A Linux rig cannot show NTFS behaviour. Recommendation: capture the gateway's encoding on the Linux rig now; declare NTFS behaviour waived for Phase 21 (render is fail-closed) and carry a Windows capture to Phase 22's C7a/C7b work. This is the one item a human decision should record.
7. **OQ-G: the prod `core` bootstrap census (C4) is not reachable from this repo.** The 47-type corpus plus the 87-type OpenAPI taxonomy plus the live rig creation of the missing allowlist types should be accepted as the census, with the table failing closed for anything else.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | build/test | yes | rustc/cargo 1.94.1 (Homebrew) | - (1.88 floor not installed locally; CI uses `stable`) |
| Docker + Compose v2 | live captures (C3b, C4 tags, C5, OQ1, OQ4, Referenced shape) | yes | Docker 29.4.0 (OrbStack), Compose v5.1.2 | none needed; captures are the only Docker consumer |
| `inductiveautomation/ignition:8.3.6` image | live captures | yes | local image present (2.31 GB, amd64 on arm64 host per compose comment) | pull |
| Phase 20 rig fixtures | live captures | yes | `crates/ignition-core/tests/fixtures/live-rig-deploy/{compose.yml,compose.bind.yml,capture.sh,mint-token.sh}`, bind mount proven viable (D11) | plain `compose.yml` + `docker compose cp` |
| `ign` on PATH | live capture commands | yes | 1.5.0 installed (not the repo build) | `cargo install --path crates/ignition-cli --force` after changing verbs |
| `gsd-tools query package-legitimacy` | package audit | NO ("Unknown command") | - | manual crates.io API audit (done above) |
| Windows host | Windows filesystem behaviour (C5, NTFS) | NO (not in this phase) | - | CI `windows-latest` runs the golden/unit tests; real NTFS behaviour waived/carried to Phase 22 |
| `uv` | sanitization/extract scripts (Python) | not probed | - | `python3 -I` for stdlib-only scripts (used throughout this research) |

**Missing dependencies with no fallback:** none for planning. **With fallback:** package-legitimacy seam (manual audit done), Windows host (CI covers unit/golden only).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in), `proptest` 1.11.0, `snapbox` 1.2 + `assert_cmd` 2.2 + `predicates` 3.1, `wiremock` (not needed here) |
| Config file | none (Cargo workspace); CI = `.github/workflows/ci.yml` |
| Quick run command | `cargo test -p ignition-core --lib deploy::` |
| Full suite command | the five CI gates, verbatim (WINDOWS.md entry 4): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`, `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DRND-01 | same tree+env gives same `render_sha256` across runs; independent of read order | unit + CLI golden | `cargo test -p ignition-core --test deploy_render determinism` / `cargo test -p ignition-cli --test contract_deploy render_digest_golden` | Wave 0 |
| DRND-02 | unknown field / wrong `schema_version` / syntax error name file:line:col (unknown-field, wrong-type, wrong-version, syntax); missing/duplicate name file only | unit | `cargo test -p ignition-core --lib deploy::yamlio` | Wave 0 |
| DRND-03 | overlay replaces whole file by path; adds when no base; other envs' `modes/` ignored | unit | `cargo test -p ignition-core --test deploy_render overlay` | Wave 0 |
| DRND-04 | un-allowlisted type refused; security-properties/levels/api-token/user-source refused even if allowlisted | unit | `cargo test -p ignition-core --test deploy_render allowlist` | Wave 0 |
| DRND-05 | corpus: 178 files `write(parse(b)) == b`, the 5 known non-Gson files copied opaque and byte-identical; whole render of the sanitized corpus is byte-identical to its inputs; writer idempotence proptest | integration + proptest, runs on all 3 CI OSes | `cargo test -p ignition-core --test deploy_corpus` | Wave 0 |
| DRND-06 | rule table agrees with every corpus `resource.json` and with the committed OpenAPI extract; mint only for `Named`; keep existing; `store-and-forward-engine`/`themes` get none; deterministic | unit | `cargo test -p ignition-core --lib deploy::rules deploy::uuid` | Wave 0 |
| DRND-07 | `Embedded` anywhere, and a plain string at each OpenAPI secret path, refuse as `embedded_secret`; message names path, a canary value never appears in error text, `--json`, or stderr | unit + CLI | `cargo test -p ignition-core --test deploy_render secrets` / `cargo test -p ignition-cli --test contract_deploy embedded_secret_names_path_not_value` | Wave 0 |
| DRND-08 | case-fold, each illegal char class, trailing dot/space, reserved names, over-budget path, CR in text member, CR in a jar NOT refused; all findings reported together | unit + proptest | `cargo test -p ignition-core --lib deploy::lint` | Wave 0 |
| DSURF-01 | for every `deploy` clap leaf: a `routes()` row on `Screen::Projects`, a README Commands-table row, and every deploy slug present in the README exit table; fails if a leaf ships without them | integration | `cargo test -p ignition-cli --test deploy_family_coverage` and `cargo test -p ignition-cli --test tui_coverage` | Wave 0 |
| four-place slugs | enum <-> literals <-> README | unit | `cargo test -p ignition-core --lib error::tests` | exists (edit) |
| MCP derivation | `deploy_render` appears in the catalog, has no `confirm` property | unit | `cargo test -p ignition-cli mcp` (existing parity tests) | exists |
| stdout purity | `deploy render --json` writes only the envelope on stdout | CLI | `cargo test -p ignition-cli --test contract_stdout_purity` (extend) | exists (edit) |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --lib deploy::` (plus the touched integration test)
- **Per wave merge:** `cargo test -p ignition-core -p ignition-cli` and `cargo clippy --workspace --all-targets -- -D warnings`
- **Phase gate:** the five CI gates green locally, then green on all three OS legs, before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/ignition-core/tests/fixtures/deploy/corpus/**` - sanitized corpus (183 JSON + CSS + stand-in jars), `SANITIZE.md`, and a fixture-secret scan test
- [ ] `crates/ignition-core/tests/fixtures/deploy/openapi-8.3.6-resource-types.json` - extract (types named/singleton, secret field paths) and the script that makes it
- [ ] `.gitattributes`: `crates/ignition-core/tests/fixtures/deploy/** -text` (and `binary` for `*.jar`)
- [ ] `.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md` + `capture.sh` subcommands `c3b c4 c5 oq1 oq4 oq8`
- [ ] `crates/ignition-core/tests/{deploy_corpus.rs,deploy_render.rs}`, `crates/ignition-cli/tests/{contract_deploy.rs,deploy_family_coverage.rs}`
- [ ] Framework install: none (all dev-deps exist)

## Security Domain

`security_enforcement` is enabled (absent = enabled; `workflow.security_enforcement: true`, ASVS level 1).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | render touches no gateway |
| V3 Session Management | no | - |
| V4 Access Control | yes (indirect) | deny-list refuses trees that declare `security-properties`, `security-levels`, API tokens, user sources (self-lockout, PITFALLS 8) |
| V5 Input Validation | yes | strict typed YAML (`deny_unknown_fields`), validating `RelPath` constructor (no `..`, absolute, `\`), size caps, UTF-8 and symlink refusals, duplicate-key refusal |
| V6 Cryptography | yes (limited) | SHA-256 via `sha2`; never hand-roll a hash; no secrets generated |
| V8 Data Protection | yes | secret values never echoed (paths only); fixtures sanitized; no ciphertext committed; `--json`/stderr canary test |
| V12 Files and Resources | yes | `--out` clobber ladder, no symlink following, regular files only, no deletes |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Secret value in an error/log/`--json` (including serde/yaml messages that echo values: yaml_serde's `invalid type: string "…"` echoes the offending value) | Information disclosure | `EmbeddedSecret` carries paths only; scrub or avoid echoing values from YAML/JSON parse errors on schema-typed fields; canary property test over error `Display`, envelope, stderr |
| Embedded ciphertext or tenant/IP data committed to a public repo | Information disclosure | Sanitized corpus + secret-scan test (Pitfall 1) |
| Path traversal / symlink escape reading or writing the tree | Tampering | `RelPath` validation, refuse symlinks, canonicalize `--out` parent, never follow links |
| Agent-driven arbitrary directory write via the MCP `render --out` | Tampering | optional `--out`, empty-or-marked-directory rule (Pitfall 7) |
| Self-lockout by shipping security singletons | Denial of service | deny table, tested with the four denied types |
| Pathological JSON/YAML (deep nesting, huge, billion-laughs aliases) | Denial of service | serde_json depth limit (128), per-file size caps (16 MiB JSON, 1 MiB YAML), RawValue cost bounded by the cap |
| Nondeterministic render hiding a tampered tree | Repudiation | `render_sha256` over sorted canonical listing; no clocks/randomness |

## Surface Parity (DSURF-01): exact seams to touch

All verified by reading source this session:
- **Error slugs (four places).** `crates/ignition-core/src/error.rs`: (1) module-header exit table (exit-2 row gains `deploy_tree_invalid`, `embedded_secret`), (2) variants + `code()`, `exit_code()`, `hint()`, `endpoint()`, (3) `exit_code_mapping_enumerated` case and `EXIT_SLUG_LITERALS` (line ~1866) rows `(2, "deploy_tree_invalid")`, `(2, "embedded_secret")`, (4) `README.md` exit-code table row 2 (line ~112); `readme_exit_table_agreement` (line ~2025) parses it both directions. Land as ONE commit. Hints must name something that fixes the case; `embedded_secret`'s hint should show the Referenced shape and say values live in the gateway's secret provider (no invented command).
- **TUI row.** `crates/ignition-tui/src/routes.rs` (workspace precedent at ~line 433): add `CliRoute { path: "deploy render", mapping: Mapping::Screen(Screen::Projects) }`; `deploy` itself is a required-subcommand group and needs no row. `tui_coverage.rs` walks the live clap tree bidirectionally, so the row and the clap leaf must land in the same change. The OutOfBand set is pinned/closed (`out_of_band_rows_are_pinned`) and must not grow; the Dashboard pin (41) must not move.
- **README entry.** Commands table (`## Commands`, line ~318) row `` `ign deploy render --env E [--repo DIR] [--out DIR]` `` plus a "### Deploy — render" section (precedent: `### Workspace — the local edit loop`, line ~1343). The website reference is generated from README by `website/sync-reference.mjs`, so no separate docs edit.
- **MCP.** `mcp.rs` derives the catalog from `Cli::command()`; no `EXCLUDED_LEAVES` change, no `GUARDED_OPS` entry for `render` (Decision D). The only check: the bidirectional catalog parity test stays green.
- **New family coverage test** (the roadmap's "a family coverage test fails if any `deploy` leaf ships without them; every later verb phase keeps this test green"). Data-driven off the clap tree so Phases 22-25 are covered automatically: walk `Cli::command()` to the `deploy` subcommand; for each leaf assert (a) a `routes()` row `deploy <leaf>` mapped to `Screen::Projects` (this part under `#[cfg(feature = "tui")]`, like `tui_coverage.rs`), (b) a README Commands-table row containing `` `ign deploy <leaf>`` (via `include_str!` of README), (c) every deploy-family slug (expose a `CoreError::DEPLOY_SLUGS` const next to the enum, or a `deploy_slugs()` fn) appears in the README exit table. Keep it in its own file (`deploy_family_coverage.rs`) so README/clap parts run with `--no-default-features`.
- **Dispatch.** `main.rs`: `Commands::Deploy(DeployArgs)`, `ActionOutput::DeployRender` (two exhaustive matches: JSON arm ~line 428 and `render.rs`), local-only dispatch with `profile: None` (precedents: `e2e init` and `lint`; not `Session::resolve`). No `// guarded:` marker.
- **Envelope shape** (additive, all keys always present): `{env, gateway, mode, out, files, bytes, render_sha256, resources:[{module,type,name,files}], tag_providers, uuids_minted, opaque_files, warnings}`; `profile: null`.

## Sources

### Primary (HIGH confidence)
- `.planning/ROADMAP.md` Phase 21 and v1.6 header; `.planning/REQUIREMENTS.md` DRND/DSURF; `.planning/STATE.md`; `.planning/research/{SUMMARY,ARCHITECTURE,PITFALLS,STACK}.md` (read; STACK not re-opened)
- Design spec `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (§3, §4, §5.4, §9, §13, §15) and the spikes `…/spikes/gateway-modes/{s1..s5,lib.sh,gen_tags.sh,mint-token.sh,fixtures/**,results/{S1,S2,S4,RIG}.md}` (read)
- The corpus `…/gateway-config-spec/gw-config/` (183 JSON, 8 CSS, 4 JAR): censuses and round-trip run this session
- Live 8.3.6 OpenAPI `~/.cache/ign-deploy-rig/openapi-8.3.6.json` and `…/docs/openapi.json` (secret oneOf shape, route taxonomy)
- Codebase read: `crates/ignition-core/src/{lib.rs,error.rs,output.rs,actions/mod.rs,actions/e2e.rs}`, `crates/ignition-cli/src/{cli.rs,main.rs,mcp.rs}`, `crates/ignition-cli/tests/tui_coverage.rs`, `crates/ignition-tui/src/routes.rs`, `README.md` (exit table, Commands, Workspace section), `.github/workflows/ci.yml`, `.gitattributes`, `Cargo.toml`/`Cargo.lock`, `.planning/WINDOWS.md`, `.planning/phases/20-config-api-client/{20-PATTERNS,20-LIVE-CAPTURES,20-01-SUMMARY,20-06-SUMMARY}.md`, `crates/ignition-core/tests/fixtures/live-rig-deploy/*`
- crates.io API (yaml_serde, libyaml-rs, sha1) and `cargo tree` (sha1 absent, sha2 0.10.9, no `preserve_order`/`arbitrary_precision`) run this session
- Context7 `/yaml/yaml-serde` (library resolved, High reputation; the error/location behaviour itself was verified by execution, not by the docs snippet returned)
- Two scratch programs executed this session: `yaml_serde` strict-parse probe and the RawValue `Node`/Gson-writer corpus run (178/5)

### Secondary (MEDIUM confidence)
- IA forum threads cited by PITFALLS (deployment-mode uuids 114966, resource.json churn 111329) via the milestone research; not re-fetched

### Tertiary (LOW confidence)
- Windows/NTFS behaviours (MAX_PATH 248, reserved names, case-insensitivity): reasoned, tagged [ASSUMED]/INFERRED, captures required

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH (versions and graph checked; one MSRV claim inherited)
- Architecture: HIGH (every seam read; the writer prototype ran on the real corpus)
- Per-type rules / secret fields: MEDIUM-HIGH (corpus + live OpenAPI; types outside both need the live capture)
- Tag-file bytes, tag path mapping, hostile names, Windows filesystem: LOW-MEDIUM (capture-gated)
- Pitfalls: HIGH for the five found by direct measurement this session (public-repo leak, CR in jars, scope-G types, YAML position gaps, no floats/non-ASCII in corpus)

**Research date:** 2026-10-12
**Valid until:** 2026-11-11 (30 days; the dependency set and codebase are stable, the capture results will supersede several [ASSUMED] rows sooner)
