# Quick Task 261029-ms3: Deploy read exclusions and over-long live paths - Research

**Researched:** 2026-10-29
**Domain:** `ign deploy` read path (sftp/local transports), plan/pull/verify/apply engines, contract surfaces
**Confidence:** HIGH (every claim below was read in this session unless tagged otherwise)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Two sources: **Target** `targets/<env>.yaml` gets an optional top-level `exclude_tag_providers: [<provider name>, ...]` (makes a bootstrap pull work, no manifest yet). **Manifest** `gateways/<name>/manifest.yaml` gets the same optional field; it applies to every environment.
- **Effective set:** the union of the built-in default, the target's list and the manifest's list.
- Parsing stays strict: unknown fields still refused, an empty or blank entry is refused, and a duplicate is refused or normalized (planner decides, and states which).
- **Contradictions are refused:** a provider that is both excluded and declared (in the manifest's `tag_providers`, or present under `tags/` in the tree) is refused with `deploy_tree_invalid` (exit 2).
- **What exclusion means:** the provider's `ignition/tag-definition/<provider>/` subtree, plus any other per-provider tag type directories, is never walked or read, in `pull`, `plan`, `verify` and `apply` (including apply's live reads, snapshot and delete set). It is also never written and never deleted.
- **Excluded providers are reported, not silent:** an additive `data` key (for example `excluded_tag_providers`) in pull, plan, verify and apply output, plus a human-mode stderr line.
- Over-limit live paths that are not excluded: **pull** skips the unit with new reason `path_too_long` (added to the skip-reason table), bootstrap continues. **plan** lists the unit as `unreadable` in a new list or relation the planner names; never absent, never in the delete set; `has_changes` and `plan_sha256` must cover it. **apply** refuses before any write when an unreadable unit is declared (rendered) or is in the delete set (existing exit-6 slug, or a new one under the four-place rule); otherwise proceeds and warns. **verify**: declared unreadable = red finding; undeclared unreadable = warning. **Never** treat an unreadable unit as absent anywhere.
- `System` is always excluded without being listed; output still names it. Planner decides and documents whether it can be turned off (recommendation: no).

### Claude's Discretion
- Exact field names (`exclude_tag_providers`), slug names, and the name of the plan relation or list for unreadable units.
- How the reader detects over-limit paths (ideally while listing, before trying to read).
- Whether the Linux and local transports also apply the path limit; keep each transport correct for itself.

### Deferred Ideas (OUT OF SCOPE)
- Upgrading OpenSSH on the hosts; a Cargo.toml version bump (release.yml stamps the version from the tag); any envelope change beyond additive keys.
</user_constraints>

## Project Constraints (from CLAUDE.md)
- No worktrees sharing `CARGO_TARGET_DIR`; run cargo on the main checkout, sequentially.
- `ign` on PATH is the installed binary: `cargo install --path crates/ignition-cli --force` before any manual smoke.
- Exit codes: source of truth is README exit table + `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`.
- `thiserror` typed errors, stable slugs; `--json` on every verb; lean deps (this task adds no crate).
- CI: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` [VERIFIED: .github/workflows/ci.yml:39-42].

## Summary

The live failure is in the Windows sftp read. `SftpTransport::read` builds per-provider chains for every manifest `tag_providers` entry across the four tag types (`target_chains`, sftp.rs:1089-1103), walks them one sftp session per level (`discover`, sftp.rs:1121-1214, the 53 minutes), then `cross_check` runs a PowerShell `\\?\` enumeration and **refuses** any entry with `path_len >= LONG_PATH_LIMIT` (sftp.rs:990-995) and any host entry sftp did not list (sftp.rs:1002-1014). `MQTT Engine` reached the read because pull generated a manifest from every name under `ignition/tag-provider/` (actions/deploy.rs:612-624 → pull.rs:147-205).

Exclusion is cleanest **before the transport**: keep excluded names out of `Scope.tag_providers` (they are never chains, so never listed, enumerated or downloaded), and replace pull's whole-type discovery read of `ignition/tag-provider` with a names-only listing. After that change no production `Scope` reads a tag type whole, so transports need no exclusion logic. Writes and deletes are already bounded to rendered files plus the delete set (transport/mod.rs:308-312), and `in_scope` keeps ledger paths of a provider not in `tag_providers` out of the delete set (plan.rs:332-344, 378-383).

Over-limit handling belongs in `cross_check`: classify host entries at or over the limit as **unreadable** instead of refusing, return them in `ObservedFiles`, and let one shared helper map them to unit keys for pull, plan and verify. The plan must take unreadable units out of `entries` *and* out of the delete set. That second part is the hidden trap: `delete_set` today drops a ledger path when `!observed.mode.contains_key(&path)` (plan.rs:384), which silently treats an unreadable file as absent.

**Primary recommendation:** exclude via `Scope` construction plus a names-only provider listing. Report unreadable units in a new `data.unreadable[]` list and `summary.unreadable`, with additive `U` lines in `plan_sha256`. Gate apply with a new exit-6 slug, `deploy_unreadable`. Leave the Linux and local transports unchanged.

## Architectural Responsibility Map

| Capability | Primary tier | Secondary | Rationale |
|---|---|---|---|
| Parse/validate `exclude_tag_providers` | `deploy/target.rs`, `deploy/manifest.rs` | — | strict serde + `validate()` already collect findings |
| Effective set + contradiction refusal | new `deploy/exclude.rs`, called by `render()` and early in `deploy_pull_in` | — | render is the choke point of render/plan/verify/apply/pull fixpoint |
| Keep excluded out of reads | `Scope` construction (`transport/mod.rs:97-110`) + pull discovery | transports (none needed) | never becomes a chain |
| Detect over-limit paths | `SftpTransport::cross_check` (Windows only) | `discover` (prune 240-259 visible children) | only PowerShell sees ≥260 entries (C7a-8) |
| Unit mapping of unreadable | `deploy/plan.rs` helper | pull, verify | one definition of "unit" (`unit_key`, plan.rs:277-294) |
| Apply refusal | `deploy/apply/gates.rs::evaluate_gates` | `error.rs` | apply = `deploy_plan` + `evaluate_gates` (actions/deploy_apply.rs:184,193) |

## Findings (file:line)

### 1. Host read path
- **Tag types** [VERIFIED: deploy/layout.rs:78-85]: `pub const TAG_TYPES: [&str; 4] = ["tag-provider", "tag-group", "tag-type-definition", "tag-definition",];`. Live layout is `config/resources/<coll>/ignition/<tagtype>/<P>/...`; repo layout swaps to `tags/<P>/<tagtype>/...` (CD5, layout.rs:94-101).
- **Scope** [VERIFIED: transport/mod.rs:83-110]: fields `mode`, `types: Vec<(String,String)>`, `tag_providers: Vec<String>`. `from_manifest` filters `ignition/<tag type>` out of `types` (mod.rs:102), so tag dirs are only read per provider, except pull's discovery scope `types: vec![("ignition","tag-provider")]` (actions/deploy.rs:614-619), the only whole tag-type read in production code. Other literal `Scope {` sites: plan.rs:1181 (test), preflight.rs:387 (`ignition/system-properties`-type singleton only), tests.
- **Windows limits** [VERIFIED: transport/winps.rs:63-68]: `pub const LONG_PATH_PREFIX: &str = "\\\\?\\";` and `pub const LONG_PATH_LIMIT: usize = 240;` ("a margin keeps a path that grows by a rename from crossing it unseen"). [VERIFIED: sftp.rs:165] `pub const WINDOWS_DATA_DIR_MAX: usize = 150;`.
- **The exact live message** is raised at sftp.rs:991-994: `"{shown} has a path of {path_len} characters on the host; the sftp server cannot resolve paths of 260 characters or more, so it cannot be read"`, with `path_len = entry.full_len - LONG_PATH_PREFIX.len()` (sftp.rs:990). The same "260 characters" text also appears at sftp.rs:221 (data_dir cap) and sftp.rs:1011 (entries sftp silently dropped).
- **C7a-8 behaviour** [CITED: .planning/milestones/v1.6-phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md:8278-8280]: a directory holding an entry ≥260 chars lists **without** it, exit 0; only PowerShell `\\?\` sees it. So over-limit detection *must* come from the enumeration. sftp can only prune visible 240-259 children during `discover`.
- **Read order** (sftp.rs:1580-1656): `target_chains` → `discover` → `cross_check` (Windows, before any download) → per-file `get` → `verify_download`. `found.files` feeds `listed`; `verify_download` refuses any listed-but-not-downloaded file (sftp.rs:1467-1472), so unreadable files must be removed from `found.files` before `listed` is built.
- **`list_types`** (sftp.rs:1521-1578; local.rs:541-562) is a names-only, two-level descent with no download and no PowerShell. This is the model for a names-only provider listing.
- **TreeReader impls:** sftp.rs:1520, local.rs:605, tests/deploy_pull.rs:511 (`Counting`), tests/deploy_verify.rs:1375 (`DeadTransport`).

### 2. Target and manifest parsing
- Both structs are `#[serde(deny_unknown_fields)]` via `strict_parse` (unknown field → `deploy_tree_invalid` with line:col, pinned by manifest.rs:209-218). Semantic findings are collected into one `deploy_tree_invalid` without position (manifest.rs:79-137; target.rs:144+).
- Add `#[serde(default)] pub exclude_tag_providers: Vec<String>` to `Target` (target.rs:20-31) and `Manifest` (manifest.rs:19-38). Validate each entry with `component_problem` (deploy/mod.rs:40-54, which refuses empty, `.`/`..`, separators, NUL and a leading `.`) **plus** an explicit `trim().is_empty()` check, because `component_problem("  ")` passes. Duplicates: **refuse** with `exclude_tag_providers[i] "X": duplicate tag provider`, mirroring manifest.rs:111-113.
- `"MQTT Engine"` (with a space) passes `component_problem` (the test list includes `"Ignition 8"`, mod.rs:62).
- Contradiction: render already refuses `tags/<P>` for any P not in `manifest.tag_providers` (render.rs:360-363). The new check therefore only needs `effective ∩ manifest.tag_providers ≠ ∅` → `deploy_tree_invalid` naming `manifest.yaml` (`tag_providers[i]`), plus, for a clearer message, `tags/<P>/` or `modes/<env>/tags/<P>/` under an excluded P.

### 3. Pull, plan, apply, verify
- **Pull skip reasons** [VERIFIED: pull.rs:63-72]: `"embedded_secret"`, `"not_round_trippable"`, `"render_refused"` (prefix), `"type_without_rule"`, `"denied_type"`. Add `pub const REASON_PATH_TOO_LONG: &str = "path_too_long";`. Unreadable units must be dropped **before** `place` (pull.rs:280-292). Otherwise a unit's readable remainder renders back to itself and passes `mismatched_units` as a partial unit.
- **Pull discovery** (actions/deploy.rs:608-624): replace the discovery `read` with `list_tag_providers(core) ∪ list_tag_providers(mode)` minus the effective exclusions, then `generate_manifest`. `live_providers` (deploy.rs:554-568) goes away. This also removes one full read session set. Semantic delta: a provider directory with no file under it is now named. Document it.
- **Plan** [VERIFIED: plan.rs:129] `pub const PLAN_HASH_HEADER: &str = "ign-deploy:plan:v1";`. Canonical lines [VERIFIED: plan.rs:87-93]: `E NUL <key> NUL <status> NUL <core> NUL <live_in joined by ","> LF`, `F NUL <collection path> NUL <live sha256 or -> NUL <desired sha256 or -> LF`, `D NUL <collection path> LF`. `canonical_text` is at plan.rs:1084-1117. `has_changes = added + changed + removed > 0` (plan.rs:523).
- **Delete-set trap** (plan.rs:384): `if rendered.files.contains_key(&path) || !observed.mode.contains_key(&path) { continue; }`. An unreadable ledger file would be skipped as "absent". Check unreadable unit keys **before** this line.
- **Apply gates** (gates.rs:52-98): plan_stale → Noop when `!has_changes` → `DeployBlocked` for `ShadowsRemoval` → destructive/drift. The error envelope carries only `{code, message, endpoint, hint}` (mcp.rs:1298), so agents can only switch on `code`. That argues for a distinct slug.
- **Verify** (actions/deploy.rs:1000-1008) runs `plan(..., None, &scope)` and passes `(key, status)` of declared **tag** units only into `check_files` (verify.rs:828-862); a missing key is `files_differ` "not found among the live files". Pair/backup verify: deploy.rs:1264.
- **Apply reads**: preflight backup read (preflight.rs:814), converge master read (converge.rs:106). Both use `Scope::from_manifest`, so they inherit the exclusion with no extra code. Writes are bounded (transport/mod.rs:308-312: "no mirror-style removal of extras").

## Recommended Approach

1. **`deploy/exclude.rs` (new):** `pub const DEFAULT_EXCLUDED_TAG_PROVIDERS: [&str; 1] = ["System"];` plus `pub fn effective(target: &Target, manifest: Option<&Manifest>) -> Vec<String>` (union, byte-sorted, deduped) and `pub fn contradictions(excluded, manifest, source) -> Vec<DeployFinding>`. No opt-out for `System` (document it). Matching: exact bytes. Contradiction detection also refuses an ASCII-case-fold match (`system` declared), since NTFS is case-insensitive.
2. **Scope:** change `Scope::from_manifest(manifest, mode)` to also take `excluded: &[String]` and filter `tag_providers` (belt and braces, because contradictions are already refused). There are six call sites: actions/deploy.rs:363, 626, 1000, 1264; preflight.rs:814; converge.rs:106. Leave the `Scope` struct unchanged, which avoids touching about 26 literals.
3. **TreeReader:** add `async fn list_tag_providers(&self, collection: &str) -> Result<Vec<String>, CoreError>` (names of dirs under `config/resources/<coll>/ignition/tag-provider/`, byte-sorted, empty when absent). Implement it in sftp (via `descend` + `list_level`, as in `list_types`), in local (`probe` + `child_dirs`), and in both test fakes.
4. **Render:** call `exclude::contradictions` from `render()`; also call it in `deploy_pull_in` right after the manifest is known, **before** any host read (fail fast).
5. **ObservedFiles** (mod.rs:116-125, derives `Default`): add `pub unreadable: Vec<Unreadable>`, where `Unreadable { collection: "core"|"mode", path: RelPath, length: usize }`, empty on Linux and local. Over-limit entries in the `local` collection go into `local_units` (names only, never read).
6. **sftp `cross_check` → classify:** host entries with `path_len >= LONG_PATH_LIMIT` (files, plus over-limit dirs with no file below them) become unreadable. They are excluded from both sides of the missing/extra/size comparison. A *short* entry missing from sftp is still `transport_failed`, as today. Return the set, filter `found.files`, then build `listed`. Optionally prune visible over-limit children in `discover` (native length = sftp path length − 1 for the leading `/`).
7. **plan.rs:** add `pub fn unreadable_units(observed) -> BTreeMap<String, UnreadableUnit>` (unit key via `unit_key` over `layer ∪ unreadable paths`, so `resource.json` markers still group). A key unreadable in **either** collection is wholly unreadable: drop it from `desired/core/mode` grouping, skip its ledger paths before plan.rs:384, and emit `Plan.unreadable: Vec<UnreadableUnit { key, live_in, declared, in_ledger }>` plus `summary.unreadable`. Set `has_changes |= any(declared || in_ledger)`; an undeclared, unledgered unreadable unit behaves like `unmanaged` for `has_changes`. Append `U NUL <key> NUL <live_in> NUL <declared 0|1> NUL <in_ledger 0|1> LF` lines after the `D` lines. Keep header `v1`: a plan with no unreadable units hashes byte-identically.
8. **Gate:** in `evaluate_gates`, after plan_stale and before the Noop check (or as step 4 beside shadows), refuse when any unreadable unit is `declared || in_ledger`, using **new slug `deploy_unreadable` (exit 6)**. Non-blocking unreadable units become a plan warning, which apply already forwards.
9. **Verify:** add `ObservedState.unreadable: Vec<(String, bool)>` (key, declared). In `check_files`, a declared unreadable unit (tag **or** resource) is a red finding with code `unreadable`. An undeclared one is a warning.
10. **Reporting:** `excluded_tag_providers: Vec<String>` on `DeployPullResult`, `DeployPlanResult`, `DeployVerifyResult` and `DeployApplyResult`. Human mode prints one stderr line `excluded tag providers: <a>, <b>` in `ignition-cli/src/render.rs` (plan 1427, pull 1475, verify 1493, apply). JSON stderr stays silent; tests pin that (contract_deploy.rs:2249, 3530, 4512). Pull also prints `skipped <key>: path_too_long`.

### Files to change
`deploy/{target.rs, manifest.rs, mod.rs (pub mod exclude), exclude.rs (new), render.rs, pull.rs, plan.rs, verify.rs, apply/gates.rs, apply/preflight.rs, apply/converge.rs}`, `deploy/transport/{mod.rs, sftp.rs, local.rs}`, `actions/{deploy.rs, deploy_apply.rs}`, `error.rs`, `ignition-cli/src/render.rs`, `README.md`, tests: `ignition-core/tests/{deploy_pull.rs, deploy_plan.rs, deploy_verify.rs, deploy_apply.rs}`, `ignition-cli/tests/contract_deploy.rs`, sftp.rs unit tests (6264-6302).

## Contract Surfaces to Update
- **README:** manifest table (2333-2341) and target table (2343-2357): add `exclude_tag_providers` and the `System` default. Pull: the manifest paragraph (3085-3090), the skip table (3100-3105, new `path_too_long` row) and the data keys (3125-3130). Plan: statuses/`unreadable` list (2769-2775), data keys (2787-2796), canonical text (2809-2827, the `U` line). Apply gates (around 3284) and refusal lists (170, 3487). Verify table (3161, finding `unreadable`). Command rows 577-581 (JSON data lists). Exit table row 267 (description + slug).
- **error.rs (four-place rule plus pins):** module doc table line 15; variant + text fn (near DeployBlocked 869-876 / 1074-1081); `code()` (around 1332); `exit_code()` 6-arm (1402-1410); `hint()` (around 1839); `DEPLOY_SLUGS` (around 1936-1951) and its pin `deploy_slugs_are_pinned_and_never_a_verdict` (3560); `exit_code_mapping_enumerated` (2120, list around 2531-2544); README-agreement list (around 2692; `readme_exit_table_agreement` 2811); `deploy_slugs_have_their_exit_codes` (3407); add `deploy_unreadable_four_places` modelled on 3594. `deploy_family_coverage.rs` checks that the README carries every slug.
- **Pinned key sets:** contract_deploy.rs:1465-1478 (pull, sorted list) and around 605-617 (plan). Verify and apply have equivalents; grep `expected.sort_unstable()`.
- **Goldens:** `PLAN_GOLDEN` (deploy_plan.rs:562) and `PLAN_GOLDEN_LEDGER` (1057) **must not change**. Their recompute helper (deploy_plan.rs:564, "from the documented line format using only the serialized result") must learn `U` lines, and a new golden with one unreadable unit should be added.
- **MCP:** the catalog is clap-derived and has no output schemas (mcp.rs:905-930), so additive keys need no change there.
- **Secret canary** (contract_deploy.rs:1455-1458): new keys carry names and paths only. Keep it that way.

## Common Pitfalls
1. **Silent absence in the delete set** (plan.rs:384). An unreadable ledger file looks absent. Classify it first and test explicitly that `delete_set.files` excludes it and that `in_ledger: true` blocks apply.
2. **Partial units.** A unit with one over-limit file still has its readable files in `observed`. If they aren't removed, plan says `changed`/`same` and pull writes a partial unit that round-trips. Drop the whole unit everywhere.
3. **Over-limit dirs drop their whole subtree from sftp.** Only the PowerShell enumeration lists them. The cross-check "missing" branch (sftp.rs:1002-1014) must exempt long entries only, never short ones; the existing test `windows_read_fails_closed_when_the_listing_omits_an_entry` (sftp.rs:6264) uses a short path and must stay red.
4. **Existing long-path test flips.** `windows_read_refuses_reparse_points_and_long_paths` (sftp.rs:6279-6302, knob `enum_len`) asserts the refusal. It must become an "all entries unreadable" assertion, while the reparse half stays a refusal.
5. **`System` default and goldens.** No deploy fixture or test tree has a `System` provider. `tests/fixtures/deploy/captured/ignition/tag-*` holds only `CAP21`, and a grep of deploy tests and fixtures found no `System` provider directory. So the default changes no offline golden. Live tests (`live_deploy_*`, `live_transport_wflwvm01.rs`) may see `System` disappear from a generated manifest if the gateway keeps one on disk [ASSUMED].
6. **Pull discovery semantics.** A names-only listing now counts an empty provider dir. The `Counting` fake (deploy_pull.rs:505-528) must forward the new method; its call-count asserts (875, 973) are `0` before any read and stay valid.
7. **`has_changes` vs apply Noop.** If a blocking unreadable unit does not set `has_changes`, `evaluate_gates` returns Noop (gates.rs:71-73) and apply "succeeds" over an unread declared unit. Either set `has_changes` or place the unreadable gate before the Noop check. Do both.
8. **Windows path length arithmetic.** Use the enumeration's `full_len - LONG_PATH_PREFIX.len()` (sftp.rs:990), consistent with today. Keep the 240 threshold: it is a deliberate margin, and the live failure was exactly 240.
9. **Linux and local:** no limit applies, so `unreadable` is always empty there. Don't add a synthetic 260 rule (CONTEXT discretion).

## Validation Architecture

| Property | Value |
|---|---|
| Framework | cargo test (workspace), wiremock/assert_cmd already present |
| Quick run | `cargo test -p ignition-core --lib deploy::` |
| Targeted | `cargo test -p ignition-core --test deploy_plan --test deploy_pull --test deploy_verify --test deploy_apply` ; `cargo test -p ignition-cli --test contract_deploy --test deploy_family_coverage` ; `cargo test -p ignition-core --lib error::` |
| Full gate | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |

Requirement → test map (all new unless noted):
- Parse: target/manifest accept the field, refuse blank, empty, duplicate and unknown entries (unit tests in target.rs/manifest.rs).
- Contradiction: an excluded provider in `tag_providers`, `System` declared, or `tags/<excluded>/` present → `deploy_tree_invalid`, exit 2 (deploy_render/plan tests).
- Exclusion: a local-fixture gateway with `ignition/tag-definition/MQTT Engine/...` plus target exclusion → pull, plan and verify never see it, and `excluded_tag_providers == ["MQTT Engine","System"]`. Also assert the sftp fake receives no `ls` for the excluded chain (session/ops count).
- Unreadable (sftp Windows fake, `enum_len`/per-path length knob): pull skips `path_too_long`; plan lists `unreadable` with no entry and no delete-set path; `plan_sha256` differs from the same tree without it; `has_changes` is true when declared.
- Apply: declared or ledgered unreadable → `deploy_unreadable`, exit 6, nothing written (pattern `assert_refused_before_any_write`, contract_deploy.rs:2650). Undeclared → proceeds with a warning.
- Verify: declared unreadable → red `unreadable` finding, exit 6; undeclared → warning.
- Goldens: `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` unchanged (existing).
- Slug: four-place test plus the README agreement.
- Wave 0: an sftp fake knob for per-path enumeration length (today `enum_len` sets one length for everything, sftp.rs:3182, 3414).

## Security Domain
- V5 input validation: new YAML fields go through strict serde, `component_problem` and the blank check. Provider names reach `target_chains` and the batch lines only after `safe_remote_path` (sftp.rs:1089-1095). The new `list_tag_providers` must apply the same `NameRule::Refuse` budget as `list_types`.
- Fail-closed (the project's core control): unreadable is never absent, a short missing entry is still `transport_failed`, and a blocking unreadable unit refuses apply. Tampering risk: a host that grows a path past 240 to dodge a delete is caught by `in_ledger` → refusal.
- Information disclosure: new data keys carry provider names and collection paths only, never bytes. The secret canary stays green.

## Environment Availability
cargo 1.94.1 / rustc 1.94.1 on the runner [VERIFIED: `cargo --version`]. Live re-test needs the production target (`whk-onprem-infra-iac-ign-tree/ignition/targets/production.yaml`) and ssh to 10.4.8.15 / 10.4.8.29: human checkpoint, not automatable here. No new packages, so no package-legitimacy audit applies.

## Assumptions Log

| # | Claim | Risk if wrong |
|---|---|---|
| A1 | Ignition tag provider names are matched case-insensitively by the gateway, so contradiction checks should ASCII-fold | Low: folding only adds refusals |
| A2 | 8.3.6 may or may not keep a `System` directory under `ignition/tag-provider/` on disk | None for offline tests; live pull output just names `System` as excluded either way |
| A3 | Units in the 240-259 window are treated as unreadable though sftp could read them | Conservative; matches today's refusal threshold |

## Open Questions
1. **Slug:** new `deploy_unreadable` (recommended; agents switch on `code` only) vs reusing `deploy_blocked` with a cause field (fewer README/error.rs edits, but it overloads the "core copy shadows" remediation hint). The planner should lock one.
2. **`U` line and header:** keep `ign-deploy:plan:v1` with an additive line type (recommended, goldens stable) vs bumping to `v2`, which would re-golden everything and break any externally stored hash.
3. **Exclusions in `plan_sha256`:** recommended **not** hashed. Excluded providers can only hide undeclared, out-of-scope units, which are already outside the hash as `unmanaged` or as out-of-scope ledger paths. Hashing them would change every golden because of the `System` default.

## Sources
- Codebase files read this session (paths and lines above). README.md sections at 170, 267, 577-583, 2276-2360, 2738-2830, 3062-3133, 3160-3161.
- `.planning/milestones/v1.6-phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md` C7a-8 (5093+, 8274-8280).
- No external docs needed: this is pure codebase integration.

**Valid until:** until the next change to `deploy/transport/sftp.rs` or `deploy/plan.rs`.
