# Quick 261030-8m8: Lift the CD5 refusals (root-level tags, nested UDT folders) - Research

**Researched:** 2026-10-30
**Domain:** `ign deploy` tag layout mapping (layout/render/pull/plan/apply), plus the live capture harness
**Confidence:** HIGH for the code paths (every file was read this session). MEDIUM for the capture plan, because the gateway behaviour it targets is unknown until the capture runs.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Live capture first, code second.** The code accepts a layout only when the capture proves (a) the on-disk layout the gateway writes for it, and (b) that one config scan picks up a plain file-level write, change and delete at those paths, the way C11 (23-LIVE-CAPTURES) showed for folder tags.
- **Capture rigs ("Docker rig + Windows VM"):**
  - Docker rig: an 8.3.6 rig through `ign rig`. This is the main capture: the layout bytes, plus scan pickup for write, change and delete.
  - Windows test VM `ign-win-836`: repeat over sftp as `svc-ignition-deploy` (the same transport and OS as production), including the NTFS paths of nested `_types_` folders. The IP is dynamic: read it with `az vm list-ip-addresses` after `az vm start`, and check that the pinned known_hosts entry covers it. Never use StrictHostKeyChecking=no. The auto-shutdown runs daily at 03:00 UTC.
- **The capture doc** lives in this task directory, in the style of 21/22/23-LIVE-CAPTURES, with decision IDs (for example CD5a for root-level tags and CD5b for nested UDT folders) that code comments and the README cite.
- **Data hygiene:** the hosts are disposable, no production data is used, no production tag file goes into the repo or a fixture, and fixtures are synthetic.
- **Read-only fallback:** if the layout is confirmed but a file-level write or delete is not picked up the way folder tags are (it needs a provider restart, two scans, or it is ignored), the layout is read-only: `pull` places it, `plan` diffs it, `verify` reports drift on it, and `apply` refuses with a clear exit-6 slug (an existing one, or a new one under the four-place rule) before any write whenever the run would write or delete such a unit. If the on-disk layout itself is not confirmed, the layout stays refused as today. **Read-only must be a per-layout flag in the layout mapping, not a special case spread across actions.**
- **Mapping invariants:** `collection_path` and `repo_path` stay exact inverses for both new layouts, and proptest or table tests cover the round-trip. Pull places these units instead of skipping them. Ledgered or declared units behave like any other tag unit (delete-set safety, ledger, excluded providers, unreadable handling from v1.6.1). `plan_sha256` and the plan goldens stay byte-identical for trees that contain neither layout. Contract parity covers the README tag layout table, the skip-reason table, the exit table (if a slug is added), golden/contract tests, and the secret canary.
- **Production follow-up** (re-pull WIGNVM01, then an infra PR) is a separate step after v1.6.2. The SUMMARY records it.

### Claude's Discretion
- The names: the decision IDs, the read-only flag, and the slug if one is needed.
- Whether the Windows VM pass reuses the Phase 22/23 capture harness or uses a smaller driver.
- How render treats a nested `tag-type-definition` folder: how `unary-resource.json` `files` lists relate across nesting levels, as the capture shows.

### Deferred Ideas (OUT OF SCOPE)
- Any write to production. The third CD5 refusal (a non-default collection), which stays fail-closed. MQTT Transmission long paths. A Cargo.toml version bump (the release stamps it from the tag). Re-pulling production.
</user_constraints>

## Summary

Each refusal is one `if` in `layout::tag_collection_path` (`layout.rs:179-188`). Nothing else in the pipeline special-cases these two layouts:

- `repo_path` already maps both shapes, because it does a generic type/provider swap.
- `plan::unit_key` already puts a root-level `tags.json` into the existing provider-root unit `ignition/tag-definition/<P>`, the unit that holds the `files: []` marker. It puts a nested type folder into its own unit when that folder carries a `unary-resource.json`.
- Render passes tag files through untouched and never builds or checks `unary-resource.json` `files`.
- The sftp and local readers already return both shapes (production pull found 79 such units).

So the "accept" outcome is a two-`if` deletion, plus the tests, the docs and a capture doc. The inverse property holds without further work.

The real risk is gateway behaviour. C11 proved only that a whole provider written at once loads in one scan. A file-level change or delete inside a provider that is already loaded was never captured, for any layout, folders included. Neither was override-by-folder at the provider root (a mode-collection root unit over a core root that has subfolders), nor whether the gateway reads or ignores `unary-resource.json` `files`. The capture must answer these, and it must run folder tags as a control, so that a negative result can be pinned on the new layout and not on tag reload in general.

**Primary recommendation:** Add a per-path `TagLayout` classifier with an `access()` flag in `layout.rs`. Delete the two refusals. If the capture fails CD5a or CD5b, flip that arm to `ReadOnly` and add `gates::read_only_refusal`, called next to `unreadable_refusal` in both `evaluate_gates` and `run_apply`, with a new slug `deploy_layout_read_only` (exit 6). Drive the docker capture with `capture22.sh up22 && setup22` plus a new `capture-cd5.sh` that writes its sections to this task directory. The import step uses the native `POST /data/api/v1/tags/import` (through `ign api call`), which works on both hosts.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|---|---|---|---|
| Repo path ↔ collection path, layout classification and access flag | `deploy/layout.rs` (pure) | — | The single mapping that CONTEXT names as the flag's home |
| Unit grouping | `deploy/plan.rs::unit_key` | pull, verify (call it) | Already marker-based; no change |
| Read-only refusal | `deploy/apply/gates.rs` | `apply/mod.rs::run_apply` entry check | Mirrors `unreadable_refusal` (WR-04 double guard) |
| Error slug | `error.rs` + README exit table | `deploy_family_coverage.rs` | Four-place rule |
| Live evidence | capture driver scripts + task capture doc | `#[ignore]` live tests | Capture-first discipline |

## Code paths that encode or depend on the refusals

All of these were read this session.

| # | Location | What it does today | Change |
|---|---|---|---|
| 1 | `layout.rs:179-183` | `if ty == "tag-definition" && tail.len() == 1 && matches!(tail[0], "tags.json" \| "udts.json") { return Err("root-level tags ... (CD5) ...") }` | Delete (CD5a passes or is read-only). Keep it only if the layout itself fails |
| 2 | `layout.rs:184-188` | `if ty == "tag-type-definition" && tail.len() > 1 { return Err("nested folders under tag-type-definition ... (CD5) ...") }` | Same, for CD5b |
| 3 | `layout.rs:87-107` (doc of `collection_path`), `:123-136` (doc of `repo_path`: "A path this function accepts can still be refused by collection_path ...") | Prose | Rewrite to cite CD5a/CD5b. The `repo_path` caveat goes away |
| 4 | `layout.rs:301-313` test `unexercised_tag_layouts_stay_fail_closed` | Asserts these paths are refused: `"tags/P/tag-definition/tags.json"`, `"tags/P/tag-definition/udts.json"`, `"tags/P/tag-type-definition/_types_/udts.json"` | Move those three to the accept table in `tags_map_per_cd5` (`:270-299`) and to `repo_path_inverts_collection_path` (`:315-346`). Keep `"tags/P/other/x.json"`, `"tags/P/x.json"`, `"tags/x.json"` as refusals, and rename the test |
| 5 | `layout.rs:145-155` `repo_path` | Generic `ignition/<tagtype>/<P>/<rest>` → `tags/<P>/<tagtype>/<rest>`, `parts.len() >= 4` | **No change.** It already returns these paths, so the inverse property holds as soon as #1 and #2 are gone |
| 6 | `render.rs:12-15` module doc ("(CD5)"), `:287-289` `mapped.tags` via `tag_provider` | Every `tags/` path is tag-origin, which means `scan_embedded` (`:746-747`), no rule table (`:437`, `:613`) and `lint_files` (`:106`) | No logic change. Render never builds or checks `unary-resource.json` `files` (`grep unary` in `src/` hits only `layout.rs` and `plan.rs`) |
| 7 | `render.rs:761-787` `finish` | `resource_key`: `ignition/tag-definition/P/tags.json` → name `P`, file `tags.json`; `ignition/tag-type-definition/P/A/udts.json` → name `P`, file `A/udts.json` | No change |
| 8 | `pull.rs:263-292, 323-336` | The `tags/P/tag-definition/tags.json` finding maps through `units_at` to the provider-root unit `ignition/tag-definition/P`, so the whole root unit (its `unary-resource.json` too) is skipped as `render_refused: ...` | No change. The finding disappears and the units round-trip. Production's 79 skips = 3 root units (default, Gateway, Spark) + 76 nested units. 76 distinct units means each nested folder carries its own `unary-resource.json` marker, otherwise they would collapse into the provider-root unit [inferred from the count + `unit_key`] |
| 9 | `plan.rs:390-410` `unit_key`, `:447-455` `is_tag_folder`, `:684-686` `CoreRelation::Hidden` | Nearest marker. `is_tag_folder` is true for the provider-root key too, so a mode root unit whose core root holds `tags.json` the render lacks becomes `hidden` | No change, **if** the capture confirms that override-by-folder applies at the provider root (Q3 below). If it does not, `hidden` is wrong for root units |
| 10 | `plan.rs:492-575` `delete_set` (excluded providers, then scope, unreadable, case-fold, mode-only) | Path-generic | No change |
| 11 | `apply/mod.rs:248-298` `write_set` | "Every rendered file of every `added` or `changed` unit is written, whole units". The deletes are the plan's delete set | No change. **Consequence for read-only:** when a provider-root unit is changed (say, only its `unary-resource.json`), its root `tags.json` is rewritten too |
| 12 | `verify.rs:305-321, 839-884` | Tag units by `unit_key`, compared byte-exact in the `files` check | No change. Drift is reported for the new layouts automatically |
| 13 | `ledger.rs:320` | Any `ignition/<TAG_TYPES>` path is a described type | No change |
| 14 | README `#### Tags layout` (`README.md:2545-2569`, the paragraph "Three layouts were not exercised ... and a non-default collection") | Docs | Rewrite: two layouts accepted (CD5a/CD5b, or read-only), the non-default collection still refused. Add root and nested examples to the mapping text |
| 15 | README skip table (`README.md:3203-3205`, the `render_refused` row) | Its examples do not name CD5 | Leave it, or add "(root-level tags and nested type folders are no longer refused, CD5a/CD5b)" |

**Pull tests:** `deploy_pull.rs:258` and `:378` assert `render_refused` for unsafe names and `bad:name`. They are **not** CD5-specific and need no change. `deploy_render.rs:589` `tags_map_to_collection_paths` gets new rows.

## Unit model for the two layouts

- **Root-level tags (CD5a).** The gateway's provider root `ignition/tag-definition/<P>/unary-resource.json` already exists. Its captured bytes are `"files": []` with `"scope": "G"` and `"attributes": {"config": {}}` (`tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json`, read this session). A root `tags.json` or `udts.json` joins that same unit (`unit_key` finds the marker in its own directory). Nothing collides. The unit gains members, and the expected marker becomes `files: ["tags.json"]` or `["tags.json","udts.json"]` [ASSUMED: by analogy with the `FolderA` marker, which lists `"tags.json", "udts.json"`. The capture confirms it]. Existing trees whose root unit holds only the marker are byte-identical, so `plan_sha256` does not move.
- **Nested type folders (CD5b).** By analogy with the `tag-definition` folders, `[P]_types_/A/B/T` should land at `ignition/tag-type-definition/<P>/A/B/udts.json`, with `A/B/unary-resource.json` beside it [ASSUMED. CD5 records that `_types_` itself never appears as a directory, so the provider root is `_types_`]. Each marked folder is its own unit, `ignition/tag-type-definition/<P>/A/B`. Open for the capture: does a folder that holds only sub-folders (`A/`) get a marker with `files: []`, or nothing? Either way `unit_key` still works: no files means no unit.
- **Fewest-change model:** no new unit concept. The marker-based `unit_key` already gives the right identity, and the collection↔repo swap is already bijective for any tail.

## Read-only fallback design (used only if the capture fails a layout)

**Flag location (layout.rs):**

```rust
// Sketch: names are Claude's discretion. CD5a/CD5b are the capture-doc IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagLayout { ProviderRootMarker, RootLevelTags, Folder, TypeRoot, NestedTypeFolder, Other }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAccess { ReadWrite, ReadOnly }
impl TagLayout {
    pub const fn access(self) -> LayoutAccess {
        match self {
            TagLayout::RootLevelTags => LayoutAccess::ReadWrite,    // CD5a: flip to ReadOnly if the capture fails
            TagLayout::NestedTypeFolder => LayoutAccess::ReadWrite, // CD5b
            _ => LayoutAccess::ReadWrite,
        }
    }
}
/// Classify a collection path (`ignition/<tagtype>/<P>/<rest>`); None for non-tag paths.
pub fn tag_layout(collection: &RelPath) -> Option<TagLayout>
```

Classify per file. `RootLevelTags` is a `tags.json` or `udts.json` with tail length 1 under `tag-definition`. `NestedTypeFolder` is any `tag-type-definition` tail longer than 1. The provider-root `unary-resource.json` stays `ProviderRootMarker` (ReadWrite), so a first deploy that creates a provider (C11's shape) is never refused.

**Gate (apply/gates.rs):** add `pub fn read_only_refusal(plan: &DeployPlanResult) -> Option<CoreError>`, modelled on `unreadable_refusal` (`gates.rs:41-57`). It collects the keys of every entry that meets either test:
- the entry's status is `Added` or `Changed`, and one of its files has `desired_sha256.is_some()` and a `ReadOnly` layout (this mirrors `write_set`, which writes whole units); or
- a `plan.delete_set.files` path has a `ReadOnly` layout. Map it to the entry whose `files` contain it.

The keys are byte-sorted and deduplicated. Call it right after `unreadable_refusal` in `evaluate_gates` (`gates.rs:100`) and in `run_apply` (`apply/mod.rs:806`): the same WR-04 double guard, and both run before any run directory, lock or write. Do not add a plan field: `PlanEntry` JSON keys are contract, and a plan `warnings` line is enough. Warnings are excluded from `plan_sha256` (`plan.rs` module doc, D-08).

**Slug:** no existing exit-6 slug fits. `deploy_blocked` (`error.rs:870-876`) has a fixed message, "a core copy shadows them", and a hint naming the core copy. `deploy_unreadable` (`error.rs:878-893`) names the sftp server and path length. Both would mislead. Add **`deploy_layout_read_only`** (exit 6). These are all the places that must change; `deploy_unreadable` touched each one:
1. The `CoreError::DeployLayoutReadOnly { keys }` variant and its text fn (`error.rs` near `:886`, `:1100`)
2. `code()` (`:1360`)
3. The exit-6 group in `exit_code()` (`:1431`)
4. `hint()` (`:1874`). Per the `deploy_unreadable_four_places` precedent, the hint must not contain `"ign "`
5. `DEPLOY_SLUGS` (`:1974-1991`)
6. The `exit_code_mapping_enumerated` case (`:2659-2665`) and `EXIT_SLUG_LITERALS` (`:2681`, `:2746`)
7. The module header table (`:15`)
8. A `deploy_layout_read_only_four_places` test (pattern at `:3728-3758`)
9. The README exit table row 6 (`README.md:268`, both the description and the slug list; pinned by `readme_exit_table_agreement` at `:2857`)
10. The README apply refusal lists (`:170`, `:582`, `:3400-3412`, `:3628`)
11. `crates/ignition-cli/tests/deploy_family_coverage.rs:289-294` (`DEPLOY_SLUGS.len() == 16`, which becomes 17, plus its message)

## Capture method

### Docker rig (main capture)
- **Bring-up:** `bash crates/ignition-core/tests/fixtures/live-rig-deploy/capture22.sh up22`, then `setup22`. This gives a fresh bind dir `$HOME/.cache/ign-deploy-rig/data/config/resources`, the compose project `ign-deploy-rig` on `127.0.0.1:18290`, a minted `cap-a` token in `$HOME/.cache/ign-deploy-rig/tokens.env` (0600), and `deploymentMode` production. Teardown: `docker compose -p ign-deploy-rig down -v`.
  - `ign rig` takes one `compose_file` per rig (README:862-871). It cannot express the `compose.yml` + `compose.bind.yml` overlay or the `IGN_DEPLOY_RIG_MODE_ARG` recreate. The locked "through `ign rig`" therefore has to mean this disposable rig. `ign rig --rig <name> status|logs`, with a `[rigs.deploy-rig] compose_file=.../compose.yml, project_name="ign-deploy-rig"` entry, can still observe it (Open Q1).
- **Driver:** write `capture-cd5.sh` beside `capture23.sh`, reusing its helpers:
  - `assert_local`, the `cap`/`shcap` block writers that stamp the command and the UTC time and refuse token values, `scan23` (wait for idle, POST, wait for `lastScanTimestamp` to advance), `chown23`, and the `c11probe`/`c11_ok` pattern;
  - set `CAPTURE_OUT` to this task's `261030-8m8-LIVE-CAPTURES.md`. Pitfall: `capture23.sh`'s default `OUT` points at `.planning/phases/23-...`, which is now archived under `milestones/`.
- **Gateway-written bytes:** use the native import `ign --profile <p> --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5 --query type=json --query collisionPolicy=Abort --header 'Content-Type: application/octet-stream' --data '{"tags":[...]}'`. This is what `capture22w.sh:744-753` did. It needs no WebDev, so the same command works on the VM.
  - Wait about 20 s: CD4 measured the flush as asynchronous, 5-15 s.
  - Then take the sorted `find` listing and the per-file `cat` and `shasum`, as C21-OQ1/OQ4 did. Imports land in `core/` (CD5: "all under core/").
- **Read-back:** `GET /data/api/v1/tags/export?provider=CAP5&type=json` (C11 shape: provider-root object with `tags`, `_types_` as a Folder), plus `resources/list/ignition/tag-provider`.
- **Live tests to reuse after the code lands:** `live_deploy_plan.rs` (pull→render→plan empty, and verify green then red), with `IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=$IGN_TOKEN_CAP_A IGN_DEPLOY_RIG_DATA=$HOME/.cache/ign-deploy-rig/data IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture`.

### Windows VM `ign-win-836`
- `az vm start -g RG-IGN-CLI-WIN-TEST -n ign-win-836`. Then read the IP: `az vm list-ip-addresses -g RG-IGN-CLI-WIN-TEST -n ign-win-836 --query "[0].virtualMachine.network.publicIpAddresses[0].ipAddress" -o tsv`.
- `source ~/.config/ign-azure-win/env`. It defines only `IGN_WINVM_HOST`, `IGN_WINVM_KEY` and `IGN_WINVM_PROFILE`, and `IGN_WINVM_HOST` may be stale. Override it with `IGN_WFLWVM01_HOST=<ip>`, and set `IGN_WFLWVM01_KEY=$IGN_WINVM_KEY` and `IGN_WFLWVM01_PROFILE=$IGN_WINVM_PROFILE`.
- **Pinned host key:** `~/.cache/ign-wflwvm01/known_hosts` (0600) has one `ssh-ed25519` entry, for `52.154.75.231`. If the new IP differs, the driver refuses. Re-pin only after `ssh-keyscan` matches `SHA256:Izw8z+3YPObb46VCy/CmWITrfunmruC2TuytpQhfOPA`, as read through a trusted channel (`az vm run-command`), per 23-LIVE-CAPTURES "Windows capture host".
- **Driver:** add a `cd5w` subcommand to `crates/ignition-core/tests/fixtures/wflwvm01/capture22w.sh`, or write a small sibling script, either way reusing its hardened `OPTS` (StrictHostKeyChecking=yes against `$PIN`) and its helpers `scap` (sftp), `rcap` (ssh cmd), `pscap` (PowerShell) and `icap` (ign).
  - Its `OUT` defaults to the archived 22 path, so set `CAPTURE_OUT`.
  - The account has Modify on `production` only, and RX on the rest of `resources`. Plain writes go into `/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/...`.
  - Scan with `ign api call --method POST --path /data/api/v1/scan/config`.
- **Live test to reuse after the code lands:** `live_transport_wflwvm01.rs::wflwvm01_pull_render_plan_is_empty` (`IGNITION_LIVE_WFLWVM01=1 ... --ignored --test-threads=1`).

### Minimal capture steps (both hosts, provider `CAP5`, synthetic names)
1. **Create via import** (gateway-written bytes, for the layout proof): root atomic tags `R1` (Int4 memory) and `RInst` (UdtInstance of `A/B/T`) at the provider root; folder `F` with `F1` (the control); `_types_/T0` (flat), and `_types_/A/B/T` with member `M1`. Record the `find` listing, every file's bytes, the root `unary-resource.json` `files` in both `tag-definition` and `tag-type-definition`, and whether `A/` (which holds only sub-folders) gets a marker. Copy the gateway-written synthetic bytes into **`tests/fixtures/deploy/captured-cd5/`** (not `captured/`, see Pitfall 6).
2. **Override probe (Q3):** write `production/ignition/tag-definition/CAP5/{unary-resource.json,tags.json}` (root unit only, `R1` value changed) while the core root and `core/.../F/` stay. One scan, then export: is `R1` changed, and is `F/F1` still present? Repeat for `production/.../tag-type-definition/CAP5/A/B/` only.
3. **Change:** edit `R1`'s `defaultValue` in the `production` root `tags.json`, edit `T`'s `M1`, and edit `F/F1` (control). One scan, then export. Record whether each changed. If not: poll every 5 s for 30 s, then a second scan (the C11b/c ladder).
4. **Delete:** remove the root `tags.json` and rewrite the root marker to `files: []` (the consistent deploy case). Separately, remove the root `tags.json` and leave `files: ["tags.json"]` (the inconsistent case: does `files` matter?). Remove `A/B/` whole. One scan each, export, and check the log for WARN (`ign logs --min-level warn --since 4min`).
5. **Round-trip (after the code change, as verification):** start from the gateway-written state of step 1, then `ign deploy pull --env <rig env>`, `ign deploy render`, then `plan` must report `has_changes: false` and must not skip the root or nested units. On the VM, also check that the nested `_types_` NTFS paths stay under `LONG_PATH_LIMIT`.
6. **Cleanup:** delete the provider (`ign tags provider delete CAP5 --yes`), scan, and confirm it is gone.

A layout **passes** when step 1 shows the expected layout and steps 3 and 4 take effect after exactly one scan, with no WARN. A layout fails as **read-only** when step 1 is confirmed but 3 or 4 need a restart, a second scan, or are ignored. A layout stays **refused** when step 1's layout is not what the mapping assumes.

## Common Pitfalls

1. **plan_sha256 stability.** The canonical text (`plan.rs` module doc) is unchanged, and the goldens (`deploy_plan.rs:577` `PLAN_GOLDEN`, `:1084` `PLAN_GOLDEN_LEDGER`, `contract_deploy.rs:25` `RENDER_GOLDEN` for the tracer) hold no tags. Keep it that way: no new E/F/D/U line fields and no new hashed plan fields. If they still pass unchanged, the "byte-identical for trees without the layouts" invariant holds.
2. **CD4 bytes.** Gateway tag files are Gson pretty-print, with no trailing newline, HTML-safe `\u00xx` escapes, `1.0E10` floats and member order kept as written. Never re-serialize. Pull keeps the bytes, and render passes them through the Gson gate (`classify`). Synthetic fixtures must be CD4-shaped, or they become `opaque_files`.
3. **Case-insensitive names (CD6).** The gateway refuses `case` next to `Case`. `lint_files` refuses sibling case-fold collisions, files and directories alike (`lint.rs:403-404`), and the delete set skips case-fold matches. **New edge:** with root files allowed, a folder literally named `tags.json` or `udts.json` would produce a file and a directory at the same path (`P/tags.json` and `P/tags.json/tags.json`). The sibling lint does not catch an exact same-name pair. Probe in step 1 whether the gateway accepts a folder named `tags.json` [ASSUMED: it refuses dotted names. CD6 shows only that `trail.` is refused]. If it accepts one, add a render finding.
4. **NTFS budget.** `LONG_PATH_LIMIT = 240` (`transport/winps.rs:71`) is measured on the absolute host path. Render refuses at `WINDOWS_MAX_PATH = 248` (`lint.rs:265`) over `windows_prefix_len` = data dir (51) + `\config\resources\` (18) + mode. For provider `default` in `production`, `...\production\ignition\tag-type-definition\default\` is about 117 characters. Adding `\unary-resource.json` leaves about 103 characters for the whole nested folder chain before a unit becomes `path_too_long`/`unreadable` (between 240 and 247 it renders but is unreadable). Deep `_types_` nesting will hit this. The v1.6.1 handling already covers it (`unreadable` units, `deploy_unreadable`), and the README can state the budget.
5. **Excluded providers.** `exclude::excluded_provider` (`exclude.rs:122`) and render's excluded-provider refusal are path-generic over `TAG_TYPES`, so the new layouts under `System` or a listed provider are never read or written. Add one test: a root-level `tags.json` under an excluded provider is still refused in render and never deleted from the ledger.
6. **Captured-fixture count pin.** `deploy_corpus.rs:110-131` `captured_tag_files_follow_cd4` asserts `files.len() == 12` for `fixtures/deploy/captured/`. Put new gateway-written synthetic bytes in a sibling dir with their own CD4 test, or update the count on purpose. `deploy_fixtures.rs` scans all of `fixtures/deploy/`: no CR, no real IPs or GUIDs, no Embedded.
7. **Secret canary.** Tag-origin files get only `scan_embedded` (`render.rs:746-747`), and plan diffs withhold secret-bearing tag files (`plan.rs:1228`, test `deploy_plan.rs:1889`). Both key on `TAG_TYPES`, not on the layout, so the new layouts are covered. Add one render test (an Embedded object in a root `tags.json` and in a nested `udts.json` gives `embedded_secret`, and the canary string never appears in the error or Debug output) and one pull test (the unit is skipped `embedded_secret`).
8. **`unary-resource.json` `files` is never validated.** A hand-authored root unit with `tags.json` but `files: []` renders fine. If capture step 4 shows the gateway honours `files`, add a render finding for tag units: the marker's `files` must equal the sibling members. Pull trees are always consistent, so this only guards hand edits.
9. **Read-only granularity.** Apply writes whole units (`write_set`). A read-only root layout therefore blocks every change to that provider's root unit, its marker included. Classify per file, but gate per unit on whether any written or deleted file is read-only. Never make the provider-root marker read-only, or a first-deploy provider create (C11) breaks.
10. **Build hygiene.** No worktrees, because of the shared `CARGO_TARGET_DIR` (CLAUDE.md). Before any manual smoke run, `cargo install --path crates/ignition-cli --force`: `ign` on PATH is 1.6.1 (verified this session). The repo root has an untracked `svc-ignition-deploy@52.154.75ace-uat4.ps1`. Do not commit it.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| docker + compose v2 | Docker rig | binary ✓, **daemon not running** (`docker info` failed) | 29.8.2 / compose v5.5.0 | Start Docker Desktop or colima before `up22` |
| az CLI | VM start and IP lookup | ✓ | 2.84.0 | — |
| ssh / sftp / jq / perl / python3 | drivers | ✓ | system | — |
| ign | capture blocks | ✓ (installed 1.6.1) | 1.6.1 | Reinstall from the repo after the code change |
| cargo | build/tests | ✓ | 1.94.1 | — |
| VM `ign-win-836` | Windows pass | unknown (it deallocates daily at 03:00 UTC) | 8.3.6 | Docker capture alone, if the user accepts (Open Q2) |

## Validation Architecture

| Property | Value |
|---|---|
| Framework | cargo test (built-in harness), plus `proptest = "1.11.0"` in `crates/ignition-core/Cargo.toml:42` |
| Quick run | `cargo test -p ignition-core --lib deploy::layout && cargo test -p ignition-core --test deploy_render --test deploy_pull --test deploy_plan` |
| Full suite | `cargo test --workspace` |

| Requirement | Test | Type | File |
|---|---|---|---|
| Both layouts map, and `collection_path`/`repo_path` invert | extend `tags_map_per_cd5` and `repo_path_inverts_collection_path`, add a proptest over `tags/<P>/<tagtype>/<seg>{1..6}` | unit | `layout.rs` ✅ |
| Remaining refusals stay | the renamed `unexercised_tag_layouts_stay_fail_closed` | unit | `layout.rs` ✅ |
| Render places both | extend `tags_map_to_collection_paths` | integration | `tests/deploy_render.rs` ✅ |
| Pull keeps root and nested units, round-trip | new test: observed root `tags.json` + nested `A/B/{udts,unary-resource}.json`, `skipped` empty, rendered == live | integration | `tests/deploy_pull.rs` ✅ |
| Unit identity | `unit_key` cases: root `tags.json` → `ignition/tag-definition/P`, nested → `.../P/A/B` | unit | `plan.rs` tests ✅ |
| Goldens unchanged | the existing `plan_sha256_golden`, `plan_sha256_golden_with_ledger`, `RENDER_GOLDEN` | integration | ✅ (must pass unchanged) |
| Secret canary | new embedded root/nested cases | integration | `deploy_render.rs`, `deploy_pull.rs` ✅ |
| Read-only (only if needed) | gate tests next to `unreadable_*` in `gates.rs`, the four-place test, `deploy_family_coverage` count, README agreement | unit | ❌ Wave 0 if needed |
| Live | `live_deploy_plan`, `wflwvm01_pull_render_plan_is_empty` | live `#[ignore]` | ✅ |

## Security Domain

V5 input validation is the relevant category: render is fail-closed on paths (`RelPath::new`, the Windows lints, case-fold). V4 access control: excluded providers are never written. The added threat is a hand-authored root or nested file used to write gateway-owned tag data. It is bounded by the manifest `tag_providers` allowlist and the exclusions, which this change leaves alone. The secret threat is covered by `scan_embedded` (Pitfall 7). The capture must never print the token or the key: the existing `cap`/`shcap`/`run_block` refuse output that contains them.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|---|---|---|
| A1 | The gateway writes root tags as `ignition/tag-definition/<P>/tags.json` (+ `udts.json`) and updates the root marker's `files` | Unit model | Layout refused, not read-only |
| A2 | Nested types land at `tag-type-definition/<P>/A/B/{udts,unary-resource}.json` | Unit model | Same |
| A3 | Each nested folder has its own marker (inferred from the 76 skipped units) | Code paths #8 | Units collapse into the provider root, so plan and verify keys change |
| A4 | Override-by-folder applies at the provider root | Code paths #9 | `hidden` misreports root units |
| A5 | The gateway refuses a tag folder named `tags.json` | Pitfall 3 | A file/dir path conflict needs a render finding |

## Open Questions

1. **"Through `ign rig`":** `ign rig` cannot bring up the bind-mount overlay. Recommendation: `capture22.sh up22/setup22`, with `ign rig status/logs` used only to observe. The planner should note this as a deviation, or confirm it with the user.
2. **VM availability:** if `ign-win-836` cannot be started or re-pinned, is a Docker-only capture enough to ship v1.6.2? CONTEXT says both hosts. Recommendation: block on the VM.
3. **Does the gateway honour `unary-resource.json` `files`?** Capture step 4 answers it, and the answer decides Pitfall 8.

## Sources

Primary (read this session): `deploy/layout.rs`, `render.rs`, `pull.rs`, `plan.rs`, `verify.rs`, `ledger.rs`, `lint.rs`, `apply/gates.rs`, `apply/mod.rs`, `transport/winps.rs`, `error.rs`; tests `deploy_render.rs`, `deploy_pull.rs`, `deploy_plan.rs`, `deploy_corpus.rs`, `deploy_family_coverage.rs`, `contract_deploy.rs`, `live_deploy_plan.rs`, `live_transport_wflwvm01.rs`; drivers `capture.sh`, `capture22.sh`, `capture23.sh`, `capture22w.sh`, `compose*.yml`; `21-LIVE-CAPTURES.md` (C21-OQ1, OQ4, CD4, CD5, CD6), `23-LIVE-CAPTURES.md` (Windows host, C11, live gates); README tag-layout, skip, exit and rig sections; `RELEASING.md`.

## Metadata
- Code paths: HIGH. Capture outcome: LOW until it runs. Read-only design: HIGH (it mirrors `deploy_unreadable` exactly).
- Valid until the next change to `layout.rs` or `apply/gates.rs`.
