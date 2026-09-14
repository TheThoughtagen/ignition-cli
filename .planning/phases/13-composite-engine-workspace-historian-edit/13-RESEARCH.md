# Phase 13: 13-composite-engine-workspace-historian-edit - Research

**Researched:** 2026-09-14
**Domain:** Workspace checkout over a generalized member-diff engine; tag↔historian bindings (spike-gated); `ign edit` kubectl-style round-trip — all on the existing Rust/ratatui/WebDev ignition-cli codebase
**Confidence:** HIGH on codebase ground truth (every claim file:line-verified today) · HIGH on the historian wire-shape correction (official 8.3 docs, Context7) · MEDIUM on spike-internal outcomes (deliberately unresolved — the roadmap mandates the spike decide)

<user_constraints>
## User Constraints (binding — no CONTEXT.md exists; source: ROADMAP.md Phase 13 flags + the locked user decision)

### Locked (roadmap flags, verbatim obligations)
- **MANDATORY SPIKE for the historian slice** — confirm licensed-Historian rig access BEFORE phase planning begins (**done — see below**); plan 01 is the time-boxed Designer-diff (re-read 05-06 artifacts; both-rig diff). The roadmap deliberately holds the historian done-definition loose pending the spike.
- **USER DECISION (locked, confirmed 2026-09-14):** Historian rig access = CONFIRMED via the resettable 2h trial window (same mechanism as the 11-06 ops learning: "image trial = 2h — budget a full live-suite run inside one window; commissioning a fresh rig costs ~4 min"). Trials reset as necessary. Implication for planning: historian spike + live-gate work must be budgeted INSIDE 2h trial windows; SC-4 targets real binding closure (spike still decides via Designer-diff).
- **Workspace and edit slices: NO research-phase beyond the spike.** Adversarial-$EDITOR and bijection/manifest pitfalls get plan-level verifications (property tests, editor-fixture harness as plan deliverables).
- Roadmap ordering: workspace hardens decode/encode at tree scale **before** edit rides the same codec leg.

### Success criteria that constrain planning (ROADMAP verbatim)
1. `ign workspace checkout` into a local tree, drift seen via `status`, push guarded by `--yes`
2. Workspace path mapping is **injective and hostile-name-safe (property tests pass)**; `--decode-scripts` checkout produces nvim-editable script files that encode back cleanly; **tag values never appear in the workspace tree**
3. Tag↔historian data-flow bindings visible in `tags config` output **on a licensed gateway**
4. Historian binding resolved honestly per the spike outcome: binding create/update works via the tag write path on a licensed rig, OR "documented limitation, now with Designer-diff evidence" — **both legitimate done states; spike outcome recorded either way**
5. `ign edit`: fetch → decode → `$EDITOR` → encode → push, with unchanged saves detected (content-hash no-op), invalid encodes refused (fail-closed), stale pushes blocked

### Requirements (REQUIREMENTS.md verbatim)
- **IDE-04**: `ign workspace checkout/status/push` over the generalized MemberSource diff engine, injective path mapping, manifest three-way compare, `--yes` push guard
- **TAGS-13**: view tag↔historian data-flow bindings through `tags config` output on a licensed gateway
- **TAGS-14**: create/update tag↔historian data-flow bindings via the tag write path — spike-gated: Designer-diff determines the wire shape; "documented limitation, now with diff evidence" is a legitimate done state if closure is not achievable
- **IDE-01**: `ign edit` — fetch → decode → `$EDITOR` → encode → push round-trip with content-hash no-op detection, fail-closed encode validation, and staleness check before push

### Prior decisions that bind this phase (STATE.md)
- Phase 8: Three-Place slug rule is EXECUTABLE (exit enum + literal (exit,slug) table + README include_str! agreement test) — any new slug lands atomically in one commit; OutOfBand taxonomy pre-declared `mcp`/`lsp`/`edit` — **the `edit` OutOfBand row lands TOGETHER with its clap command** (orphan rows fail the clap walk by design; pinned test set is currently exactly `["completions", "api call"]`); stdout-purity byte-scan harness is assert-based over the real binary
- Phase 11 / 11-02: ROUTE_BUNDLE_VERSION atomic bump pattern (five route constants + mod.rs + VERSION in ONE commit, three-way pin test); base64 envelope carrier (`payload_b64`/`file_b64`); transfer seam is base64-only
- Phase 10: guard-ladder patterns (`build_blast_radius` → render → `require_confirmation` composed as ONE gate fn — 10-04's `preview_then_confirm`); live gates run in-phase, env-gated, evidence in `{phase}-LIVE-GATE.md`
- Phase 12: everything TUI-styled via `theme::Palette` tokens (CI grep enforces)
- Phase 11 ops: image trial = 2h per window; fresh rig ~4 min; **unique provider names per run**; measurement-backed async tolerances; LIVE_GATE serializer for mutating gates

### Deferred (out of scope this phase)
- Transports (MCP/LSP) — Phase 14
- File watching for the workspace (`notify` rejected at STACK level — checkout/sync is command-driven by design)
- Single-resource edit as a separate wire surface (see Architecture: edit rides the whole-tree codec; the resource-path argument scopes the editor, it does not add a new push path)
</user_constraints>

## Summary

Phase 13 is three slices sharing one seam. The codebase reading confirms the roadmap's bet: **everything the workspace and edit slices need already exists in pure, unit-testable form** — the member surgery + diff engine in `crates/ignition-core/src/client/resources.rs` (list/read/replace/remove primitives, FNV-1a `member_hashes`, descriptor-normalizing `diff_members`, `project_meta_delta`), and the full decode/encode codec leg in `crates/ignition-core/src/client/scripts_codec.rs` (`flint_encode`/`flint_decode`, `decode_export_tree` zip→dir with `scripts-manifest.json` + counter-named `.py` sidecars, `encode_export_tree` dir→zip with span-splice re-encoding and byte-exact unedited round-trip). "Generalize into MemberSource" is a ~100–200 line refactor (ARCHITECTURE.md's estimate, confirmed by reading the code): extract a source trait/enum (zip bytes vs fs directory) over the member-enumeration/hash/read surface that today only speaks `&[u8]` zip bytes. The wire layer needs **zero new WebDev routes** — workspace and edit both ride the native-REST `project_export_to_file` / `project_import` pair (`actions/resources.rs:150`, `actions/projects.rs:579`).

The historian slice's mandatory spike got a material de-risk during this research: cross-checking the official 8.3 docs (Context7 `/websites/inductiveautomation_8_3`) against the 05-06 spike artifact revealed a **property-name discrepancy**. The 05-06 spike configured the tag with `historicalProvider: "p5hist"` (e2e_webdev.rs:995); the official 8.3 Tag Properties table names the Storage-Provider property **`historyProvider`** (plus `historyEnabled`, `sampleMode: OnChange|Periodic|TagGroup`, the Historical Tag Group property, `historicalDeadbandMode`, `historicalDeadband`). A misnamed key on the tagConfig route's configure path would be silently tolerated while never binding — which matches the spike's observed "config accepted, query null" outcome exactly. This does NOT replace the spike: the Designer-diff remains the mandated oracle (a Designer-written binding's `tags config get` output is ground truth and will also reveal the sample-mode/tag-group fields the docs table summarizes). But plan 01 now starts with a HIGH-confidence leading hypothesis and a field checklist instead of a blank page.

Rig ops are fully proceduralized from Phases 9–11 (headless commissioning ~4 min, API-token recipe, unique provider names, LIVE_GATE serializer), and the locked user decision confirms the licensed-Historian requirement is satisfiable inside resettable 2h trial windows. The planner's job: plan 01 = the time-boxed both-rig Designer-diff spike with the documented-limitation fallback pre-cleared; workspace plans carry the bijection property tests and manifest design; edit plans carry the editor-fixture harness, content-hash no-op, and staleness gate. Nothing in the workspace/edit slices requires new wire truth beyond what v1.0 captured.

**Primary recommendation:** Generalize `client/resources.rs` into a MemberSource abstraction (zip + fs-dir impls) and reuse `scripts_codec.rs` verbatim for tree decode/encode; run the historian spike first (plan 01) against the doc-corrected `historyProvider` hypothesis with Designer-diff as oracle; ship workspace before edit; land the `edit` OutOfBand row atomically with its clap command.

## Standard Stack

### Core (all already in the workspace graph — ZERO new runtime dependencies)
| Component | Version/Where | Purpose in this phase | Why Standard |
|---|---|---|---|
| `client/resources.rs` pure engine | ignition-core | `resource_members`/`read_member`/`replace_member`/`remove_member`/`member_hashes`/`diff_members`/`normalize_descriptor` — the v1.0 engine to generalize | Live-proven in 05-02/06-08/07-01; pure, no GatewayApi, unit-testable without a gateway |
| `client/scripts_codec.rs` codec leg | ignition-core | `flint_encode/decode`, `dedent/reindent`, `decode_export_tree`/`encode_export_tree` — `--decode-scripts` checkout AND `ign edit` ride this unchanged | Byte-exact unedited round-trip is the pinned sacred invariant (tests in-module + `scripts_codec_contract.rs`) |
| Native REST export/import | `GatewayApi::project_export_to_file` / `project_import` | The workspace checkout/push and edit fetch/push transport — no new WebDev routes | Same transport `ign resource`/`ign project diff|sync` use today |
| `zip` crate | workspace dep | Member enumeration + deterministic rewrite (`SimpleFileOptions`, deflate, order-preserving) | Already the surgery engine's substrate |
| `tempfile` | direct core dep (promoted 05-02) | Private 0700 tempdir for `ign edit`; export staging | ARCHITECTURE.md names it as the edit temp-dir base |
| `tokio::process::Command` | tokio (process feature enabled) | The `$EDITOR` spawn — `lint.rs`'s ARG-VECTOR (never shell-string) precedent at `actions/lint.rs:113-121` | Injection-safe; `ComposeRunner`-style seam for tests |
| `serde_json` | workspace dep | Workspace/edit manifests (read-only on member bytes — the no-`preserve_order` discipline holds) | scripts_codec's own manifest already rides it |
| `quick-xml`/`csv`/`lsp-*` | n/a | **Not touched this phase** | Bulk transfer + LSP are Phases 11/14 |

### Supporting (dev/test)
| Component | Purpose | Note |
|---|---|---|
| `wiremock` + REQUEST-pinning discipline | Contract tests for any new client calls (there should be ~none — no new wire surface) | 10-02/11-02 pattern |
| `snapbox` goldens + `assert_cmd` | `workspace status/push` refusal and render goldens | Existing contract test stack |
| `proptest` (NEW, dev-dependency of ignition-core only) | The injective-mapping property test ROADMAP mandates (SC-2: "property tests pass") | **Only new entry in the tree, and only as a dev-dep.** No property-test crate exists in the workspace today (grep-verified). Lean-tree-compliant because it never ships in the binary. Alternative: exhaustive hand-built adversarial-fixture tables — rejected by Pitfall 17 ("a property test, not examples") |
| Editor-fixture harness (std process spawns, in-test) | Blocking (vim-style), deferred-exit (vscode `--wait` pattern), forked (emacs daemon) archetypes as REAL spawned processes | Pitfall 15: "as spawned-process fixtures, not mocks of `std::process`" |

### Alternatives Considered
| Instead of | Could use | Verdict |
|---|---|---|
| MemberSource trait over zip/dir | Two parallel implementations | Refactor preferred — `member_hashes`/`diff_members` keep ONE implementation of normalization + status semantics |
| New WebDev route for workspace/edit | Native REST export/import | Native REST — zero bundle-bump risk; a new route is warranted ONLY if the historian spike reveals binding config outside the tag surface (then follow the 11-02 atomic-bump + byte-0 contract) |
| `proptest` | `quickcheck` | proptest: shrinking + no macro-magic arguments; standard Rust choice |
| Whole-project `ign edit` | Per-resource fetch/edit/push | Whole-tree codec path already exists end-to-end (ARCHITECTURE.md's recommendation, confirmed); the `<resource-path>` argument scopes which file the EDITOR opens, push stays the existing project-import path |

**Installation (the only tree delta):**
```bash
cargo add proptest --dev -p ignition-core
```

## Architecture Patterns

### What exists today — the exact seam inventory (codebase ground truth)

**`crates/ignition-core/src/client/resources.rs`** (PURE — zero GatewayApi, no I/O beyond `zip`):
- `member_path(user) / user_path(member)` (lines 70-100): the `<collection>/<rest>` ↔ `<collection>/resources/<rest>` mapping, with the LIVE-PROVEN root-file shape `<X>` ↔ `<X>/resources/<X>` (06-08) and its deliberate `<X>/<X>`→`<X>` alias
- `resource_members(zip) -> Vec<String>` (194), `read_member` (214), `replace_member` (372, upsert w/ descriptor merge-or-synthesize), `remove_member` (383)
- `FOLDER_DESCRIPTOR = "resource.json"` (129) + the landing rule: a new file member lands ONLY when its parent descriptor lists it — rewrite_zip handles merge/synthesize automatically (264-367)
- `normalize_descriptor` (448): strips `attributes.lastModification`/`lastModificationSignature`, canonical-sorts keys — the volatility guard that makes cross-time compares possible
- `member_hashes(zip) -> BTreeMap<String, u64>` (472, FNV-1a; descriptors hash normalized), `diff_members(zip_a, zip_b) -> MemberDiff` (554, B-relative-to-A added/removed/changed/same), `project_meta_delta` (620: title/enabled/parent)

**`crates/ignition-core/src/client/scripts_codec.rs`** (PURE):
- `SCRIPT_KEYS` (9 keys, 67), `flint_encode`/`flint_decode` (89/140 — backslash-first multi-pass encode, single-pass decode), `dedent`/`reindent` (187/237)
- `decode_member`/`encode_member` (564/606): raw-byte span scanning, splice encoding, byte-exact unedited invariant, broken-JSON encode → `InvalidInput` exit 2 (**this IS the fail-closed validation IDE-01 needs — already built**)
- `decode_export_tree(zip, dir)` (682): full tree + `<member>.<n>.py` sidecars + `MANIFEST_NAME = "scripts-manifest.json"` at root; refuses to shadow a real `scripts-manifest.json` member or a sidecar-collision
- `encode_export_tree(dir)` (774): manifest consumed+stripped, sidecars stripped, pointer re-resolution on current bytes, missing-sidecar keeps value

**`crates/ignition-core/src/actions/projects.rs`**:
- `project_diff` (426) / `project_sync` (502): the two-gateway orchestration over the pure engine — sync's selection semantics (explicit `--resource` + `--all-changed`, `--delete` opt-in for removals, zero-write honesty at 577) are the shape workspace push generalizes
- **Label reconciliation precedent (535-555):** diff speaks B-relative-to-A; promotion speaks source→target. Workspace status MUST repeat this reconciliation explicitly (status speaks gateway-relative-to-local or the reverse — pick one and pin it in tests)

**`crates/ignition-core/src/actions/tags.rs`**:
- `tags_config_get` (1407): getConfig + `reparse_stringified` — returns the FULL config object; **TAGS-13 extends this surface** (history properties ride `config` today; visibility = naming them in render/README, possibly a summary block)
- `configure_single` (1348) / `tags_config_create|edit` (1450/1463): the tag write path TAGS-14 rides — collisionPolicy 'a'/'o', basePath+name derivation via `split_base_path`
- `split_base_path` (1327), `reparse_stringified` (1298)

**Live-fixture machinery to reuse verbatim** (`crates/ignition-cli/tests/e2e_webdev.rs`):
- `provision_internal_historian` (868) / `delete_internal_historian` (899): native REST `POST/DELETE /data/api/v1/resources/com.inductiveautomation.historian/historian-provider`, self-cleaning with find-first
- `live_tags_history_historian_and_binding_spike` (964): the 05-06 spike — base shape, candidate ladder (execution scan-class keys 1095, wider windows + aggregationModes 1112-1127, `browseHistoricalTags` cross-check 1129-1148), documented-limitation fallback (1150-1165) with the Designer-diff resolution path named VERBATIM in the comment (1160-1162): *"create one history tag by hand in the Designer, getConfig it via this CLI, diff the shapes"*
- LIVE_GATE serializer (973), unique-provider-name discipline (1201-1212), pre-clean idempotency (976-980)

### Pattern 1: MemberSource — generalize the member source, not the engine

**What:** the engine's functions all take `zip_bytes: &[u8]`. Introduce a source abstraction so member enumeration/read/hash work over EITHER a gateway-export zip OR a checked-out local directory tree:

```rust
// Sketch — ignition-core/src/client/workspace.rs (NEW) or resources.rs extension.
// The engine keeps ONE diff/status implementation; sources are dumb enumerators.
pub enum MemberSource {
    Zip(Vec<u8>),                    // gateway export (today's shape)
    Tree { root: PathBuf, mapping: PathMapping }, // checked-out workspace
}
impl MemberSource {
    fn member_hashes(&self) -> Result<BTreeMap<String, u64>, CoreError>; // normalized, zip impl == existing fn
    fn read(&self, user_path: &str) -> Result<Vec<u8>, CoreError>;
    fn members(&self) -> Result<Vec<String>, CoreError>;
}
```

**When to use:** workspace `checkout` builds the Tree side by writing files; `status` compares Tree vs fresh gateway Zip; `push` reads changed members from the Tree, splices into the fresh gateway zip via existing `replace_member`/`remove_member` (descriptor landing rules ride free — 05-07's put-new hazard already handled), validates, ONE overwrite-import.
**Scope estimate:** ~100-200 lines (ARCHITECTURE.md P7 estimate, confirmed against the code — the Tree impl reuses `user_path`/`member_path` pure fns and `normalize_descriptor` verbatim).

### Pattern 2: Workspace manifest = the three-way compare state

**What:** `.ign-workspace.json` (name planner's call — MUST be dot-prefixed so it can never collide with a member-derived path; keep it DISTINCT from the codec's `scripts-manifest.json` — single responsibility, and `encode_export_tree` already refuses/strips only its own manifest). Per member at checkout: gateway user path ↔ local relative path (the mapping is RECORDED, not recomputed), FNV-1a content hash of the checkout-time bytes (descriptor-normalized for `resource.json` members), checkout timestamp, project + profile.
**Three-way compare (status):** per path — `local_hash == manifest.hash && gateway_hash == manifest.hash` → clean; local differs only → `local_edit` (pushable); gateway differs only → `gateway_drift` (pullable); both differ → `conflict` (push REFUSES — clobbering is not `--yes`-able, Pitfall 17). Additions/deletions handled by set-difference against the manifest.
**Gitignore decision (Pitfall 17):** decide and DOCUMENT manifest-committed-vs-ignored at plan level; recommended: committed (it is the workspace's identity), with `scripts-manifest.json` + `*.py` sidecars gitignored per the codec tree convention.

### Pattern 3: `ign edit` pipeline over the existing codec + an Editor seam

```
ign edit <project> [resource-path] [--yes]
  ├─ export_zip_bytes(api)                      [existing]
  ├─ decode_export_tree(zip, tmpdir)            [existing — sidecars + codec manifest]
  │    (resource-path arg: only that member + its sidecars are OPENED in $EDITOR;
  │     the whole tree still decodes so encode-back is exact)
  ├─ snapshot: member_hashes of fetched zip     [staleness baseline]
  ├─ Editor.open(target file in tmpdir)         [NEW seam trait; TokioEditor impl on
  │                                              tokio::process, no-op test impl —
  │                                              ComposeRunner/lint precedents]
  ├─ re-encode + byte-compare → unchanged = clean no-op exit (no push, no prompt)
  ├─ encode_export_tree(tmpdir)                 [existing; broken JSON → InvalidInput exit 2 = FAIL CLOSED]
  ├─ staleness: fresh export, compare target member hash vs snapshot → drift = refuse
  ├─ diff summary (member-level, from diff_members) → --yes guard ladder
  └─ api.project_import(project, zip, true)     [existing]
```

**When to use:** IDE-01. **Key decisions pre-made by the codebase:** fail-closed is `encode_member`'s existing `InvalidInput`; no-op detection is byte-compare of re-encoded output (the sacred invariant makes "unchanged" well-defined); staleness is the member-hash snapshot (NOT an etag — project exports carry no per-resource etag; re-export+hash-compare is the honest mechanism and reuses `member_hashes` unchanged).

### Pattern 4: The guard ladder composes, it does not fork

Workspace push and edit push both fold into the 10-04 `preview_then_confirm` shape: build the blast radius (the three-way/one-member diff summary) → render → `require_confirmation` → push. ONE gate function per slice means the refusal shape cannot drift per verb (the 10-04 lesson), and the refusal message IS the preview.

### Anti-Patterns to Avoid
- **Recomputing the mapping on push instead of reading the recorded manifest** — a re-derivation can diverge from checkout (schema evolution, refactors) and silently orphan files. Record path↔path at checkout; push/status trust the manifest, refuse unknown local files.
- **Modeling history properties as typed Rust structs with guessed names** — Pitfall 12 verbatim. TAGS-13/14 keep config passthrough (`Value`) + reparse; the spike's captured shape is the only source of names.
- **Letting `ign edit` reuse a shared TMPDIR** — private tempdir 0700, Drop-guard cleanup, unique per invocation (Pitfall 15).
- **New slugs or envelope keys outside the additive-only discipline** — any new exit slug lands Three-Place ATOMIC; any new envelope key is additive with skip-serialize-at-None (the `data.loss_report` precedent).
- **A second hand-rolled request site** — all wire traffic stays on `GatewayApi`; zero format-specific Readers in action code (11-04 byte-faithfulness invariant).

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---|---|---|---|
| Descriptor normalization / hash comparison | Fresh comparison logic per slice | `normalize_descriptor` + `member_hashes` + `diff_members` as-is | The `lastModification` volatility guard is live-evidenced (07-01); byte-compare without it flags identical content as CHANGED |
| Embedded-script decode/encode | Any new script extraction | `scripts_codec.rs` end-to-end | Dual-ported ignition-nvim contract, span-splice, byte-exact invariant — already contract-tested |
| Resource landing on import | Reimplementing descriptor bookkeeping | `replace_member`'s merge/synthesize (rewrite_zip) | The "import answers ok while nothing lands" bug is ALREADY closed; reusing it is the fix |
| Fail-closed encode validation | A new JSON-validity gate | `encode_member`'s existing `InvalidInput` refusal | Same error, same exit 2, one source |
| Editor process management | Shell-string spawning, exit-code-as-truth | `tokio::process` ARG VECTOR + Editor seam trait + content-hash decides | lint.rs precedent; Pitfall 15 |
| Property testing for the mapping | Ad-hoc random tests | `proptest` (dev-dep) generating hostile-name corpora | Pitfall 17: "a property test, not examples" |
| Live rig commissioning/token provisioning | Anything new | 09-02 wire recipe + `provision_token.sh` + 11-RIG-NOTES procedures | Fifth-generation-proven (~4 min to RUNNING) |
| Historian provisioning in fixtures | A new path | `provision_internal_historian` helper verbatim | Native-REST, no database, live-proven (e2e_webdev.rs:868) |

**Key insight:** Phase 13's workspace and edit slices are *orchestration engineering over a proven core*, not wire discovery. Every wire-level uncertainty is quarantined inside the historian spike.

## Common Pitfalls

### Pitfall W1: Non-injective path mapping under hostile names (SC-2's core)
**What goes wrong:** two distinct resources differing only by CASE map to one path on macOS APFS (case-insensitive default) — one file silently lost at checkout, then the push DELETES it server-side. Names with `/`, `\0`, traversal (`../`), reserved names, or unicode NFC/NFD variants break the mapping further.
**Why it happens:** checkout works on the happy-path project; the bijection is never tested so never enforced.
**How to avoid:** the mapping user-path → relative-fs-path must be explicit and injective: refuse (never last-write-wins) when two members collide post-mapping (case-fold compare at plan level, since APFS is the deployment surface); refuse traversal-shaped results (resolved path escaping the checkout root = hard error); property-test the bijection + round-trip over generated hostile corpora (case pairs, `../`, embedded slashes are ALREADY legal in member paths via folders — the mapping is segment-wise so slashes only split, never inject, but `\0`/`.`/`..` segments must refuse). Note `user_path()`/`member_path()` already handle the zip-side mapping; the NEW surface is fs-path escaping — decide the escaping scheme (e.g. percent-encode hostile bytes per-segment) in plan, pin with property tests both directions.
**Warning signs:** files "missing" after checkout on macOS; a test suite of only well-behaved names.

### Pitfall W2: Push clobbering concurrent edits (the Designer is a real concurrent writer)
**What goes wrong:** without the manifest three-way compare, `workspace push` can't distinguish "user edited locally" from "gateway changed since checkout" — it overwrites Designer/git-module edits.
**How to avoid:** the manifest IS the conflict detector (Pattern 2); status surfaces `conflict` rows; push refuses conflicts outright; the `--yes` guard gates the remaining blast radius with the diff summary as its message. Zero-write honesty: empty effective selection performs NO import (project_sync's precedent at projects.rs:577).
**Warning signs:** "my edit pushed but other things changed."

### Pitfall W3: Tag data leaking into the workspace tree (SC-2's exclusion clause)
**What goes wrong:** workspace derives from the PROJECT export zip only — tag providers are a different seam (the scope-honesty contract in ProjectDiffResult: `scope: "project"`, tags ride the provider surface). The structural guarantee: if checkout never ingests tag data, no tag value can appear. The hazard would be a future "convenience" that fuses provider export into the tree, or decode sidecars/manifest accidentally embedding values.
**How to avoid:** keep the source strictly `project_export_to_file`; assert in the checkout contract test that the tree contains only zip-derived members (+ codec sidecars + the two manifests); document the boundary in README (workspace is project-resources scope; tags remain `ign tags` verbs).
**Warning signs:** any workspace test fixture built from anything but a project export.

### Pitfall E1: Adversarial `$EDITOR` (Pitfall 15 — the plan-level verification the roadmap mandates)
**What goes wrong:** vscode/emacs fork or daemonize (exit ≠ done; need `--wait` semantics); editors write backups, don't flush, or change nothing. Treating exit code as truth reads half-written or unchanged files.
**How to avoid:** exit code advisory; content-hash comparison decides changed-vs-no-op (re-encode + byte-compare — unchanged → clean no-op exit, no push, no prompt); content-correct temp-file suffix (`.py` for sidecars, `.json` for members — highlighting AND the codec care); private tempdir 0700 + Drop-guard; on failure keep the temp file and PRINT its path; document `--wait` expectations for IDE editors in README. Fixture harness: the three archetypes as REAL spawned processes (vim-style blocking, `--wait` deferred-exit shim script, daemon-exit-immediately shim script).
**Warning signs:** round-trip only proven with vim; temp files accumulating in `/tmp`.

### Pitfall E2: Stale push clobbering (IDE-01's staleness check)
**What goes wrong:** gateway resource changed between fetch and push (Designer edit, another terminal) — the edit push overwrites it.
**How to avoid:** snapshot the fetched zip's `member_hashes` at fetch; immediately before import, re-export and compare the target member's hash; drift → refuse with "resource changed on gateway — re-run to fetch fresh" (NOT `--yes`-able by default, per Pitfall 15). Reuse `member_hashes` unchanged.
**Warning signs:** any staleness design that invents an etag the export endpoint does not provide.

### Pitfall H1: Historian guess-shapes (Pitfall 12 — the phase's mandatory spike)
**What goes wrong:** building TAGS-14 on guessed field names. **The 05-06 evidence + new doc finding:** the v1.0 spike configured `historicalProvider` (e2e_webdev.rs:995) and got "config accepted, query null" for every candidate. The official 8.3 Tag Properties table (Context7 `/websites/inductiveautomation_8_3`, Tag Properties → History Properties) names the properties: `historyEnabled` (Boolean), **`historyProvider`** (String — "Storage Provider"), `sampleMode` (`OnChange` | `Periodic` | `TagGroup`), plus Historical Tag Group (the property table row for it was not captured verbatim in this research — the Designer-diff will name it; commonly `historicalScanclass` in the JSON model), `historicalDeadbandMode` (`Absolute`|`Percent`|`Off`), `historicalDeadband`. The docs also state dataset tags are unsupported and that history needs the History section configured (enabled + storage provider + sample mode + historical tag group). A configure payload carrying `historicalProvider` (not `historyProvider`) and NO sample-mode/historical-group settings is consistent with every observed 05-06 symptom. Dataset-type history also can't bind — the spike tag is Int4, fine.
**How to avoid:** the Designer-diff spike (plan 01) is still the oracle — procedure verbatim: on BOTH trial rigs (8.3.3 + 8.3.6, per the both-rig rule for historian), with a provisioned `InternalHistorian`: (1) `tags config get` the tag BEFORE; (2) create/enable the binding by hand in the Designer (History section: enable, pick storage provider, Sample Mode = Tag Group, pick a historical group); (3) `tags config get` AFTER; (4) diff shapes — capture the full config JSON as the pinned artifact; (5) replay via `tags config create/edit` through our route; (6) write + `tags history query` → data = binding closure achieved (pin the shape in README + captures); no data after replay = documented limitation WITH the diff evidence committed (legitimate done state). Note also PITFALLS.md's secondary trap: if the diff shows binding config living OUTSIDE the tag config (provider/module settings), the replay may need more than a tag write — that finding would itself be the documented-limitation evidence (or a scoped follow-up the roadmap's loose done-definition already anticipates).
**Warning signs:** any TAGS-14 implementation committing field names that don't appear in the captured diff artifact.

### Pitfall H2: Trial-window blowouts (the locked 2h budget)
**What goes wrong:** the licensed-rig window expires mid-spike/gate; trial-state decay mid-run produces phantom failures.
**How to avoid:** budget each live activity inside ONE 2h window (11-06 ops learning); fresh rig ~4 min via the 09-02 headless recipe; unique provider names per run; LIVE_GATE serialization for mutating gates; pre-clean helpers for re-runs; trials reset as necessary (user-locked) — but NEVER count on a reset mid-activity. The Designer-diff has an extra ops wrinkle: the Designer is a desktop app launched from the gateway webpage — it runs on the host Mac against the port-forwarded containers; budget Designer install/start time inside the window, and script the before/after `tags config get` captures so the window only needs the interactive Designer step.

### Pitfall R1: Contract rituals (all plan-level, all machine-enforced)
- **`ign edit` OutOfBand row** lands in the SAME task as the clap command; the pinned `out_of_band_rows_are_pinned` test (tui_coverage.rs:144, set `["completions", "api call"]`) extends in that commit. New clap nodes (`workspace`, its verbs, `edit`) also need `routes()` rows or the bidirectional tui_coverage walk goes red.
- **Any new exit slug** = Three-Place ATOMIC (exit enum + literal (exit,slug) table + README row; the include_str! agreement test makes a split commit red).
- **Stdout purity**: `ign edit` handing the terminal to the child editor is exactly what OutOfBand declares; workspace verbs stay in the normal envelope dispatch and must survive the byte-scan harness.
- **Envelope additions** (e.g. a workspace/status result shape) are additive-only with skip-serialization at defaults (the `loss_report` precedent); pre-existing envelopes byte-identical.
- **If (and only if) the spike forces a new WebDev route** for historian binding: 11-02 pattern — five route constants + mod.rs + ROUTE_BUNDLE_VERSION bump in ONE commit, three-way pin test, wiremock REQUEST pins before consumers, byte-0 `def doPost` contract, base64 envelope carrier.

## Code Examples

### The generalized status compare (composition of existing primitives)
```rust
// Sketch: actions/workspace.rs — status = manifest vs local tree vs fresh gateway zip.
// All comparisons ride member_hashes (descriptor-normalized FNV-1a).
let gateway = MemberSource::Zip(export_zip_bytes(api, project).await?); // existing
let gateway_hashes = gateway.member_hashes()?;                           // existing logic
let manifest = read_workspace_manifest(root)?;                          // recorded at checkout
for path in union(manifest.keys(), local_files(), gateway_hashes.keys()) {
    let at_checkout = manifest.hash_of(path);        // Option
    let local_now   = tree_hash(root, &manifest.local_path(path)?); // normalized fn reuse
    let gw_now      = gateway_hashes.get(path);
    // clean | local_edit | gateway_drift | conflict — conflict REFUSES push later
}
```

### The fail-closed edit encode (already built — the plan just composes it)
```rust
// scripts_codec.rs:606 — a member the user broke in the editor refuses here:
let out = encode_member(&bytes, entries, &texts)  // → CoreError::InvalidInput, exit 2
// "the edited member no longer parses as JSON — cannot splice its scripts back"
```

### The spike's Designer-diff capture (procedure the plan encodes)
```bash
# Inside one 2h trial window, per rig (8.3.3 AND 8.3.6):
ign webdev deploy --with-script-exec                       # routes
ign profile add rig http://localhost:PORT && token recipe  # 11-RIG-NOTES
# provision historian: POST /data/api/v1/resources/com.inductiveautomation.historian/
#   historian-provider  {"type":"com.inductiveautomation.historian/historian-provider", ...}
ign tags config create '[default]P13H/T1' --file - <<'EOF'  # BEFORE shape
{"tagType":"AtomicTag","dataType":"Int4","value":0}
EOF
ign tags config get '[default]P13H/T1' > before.json
#  … Designer: enable History on T1, storage provider = the trial historian,
#    Sample Mode = Tag Group, pick historical group …
ign tags config get '[default]P13H/T1' > after.json
diff <(jq -S . before.json) <(jq -S . after.json) > designer-diff-833.txt   # COMMIT THIS
# replay: tags config edit with the diffed keys (expect historyProvider, sampleMode,
# historical group) → tags write → tags history query → DATA or documented limitation
```

### The history-property vocabulary the diff should explain (official 8.3 docs)
| Designer UI label | JSON/scripting name | Type / values |
|---|---|---|
| History Enabled | `historyEnabled` | Boolean |
| Storage Provider | **`historyProvider`** | String (provider name as written at config time) |
| Sample Mode | `sampleMode` | `OnChange` \| `Periodic` \| `TagGroup` |
| Historical Tag Group | (spike captures the exact key; commonly `historicalScanclass`) | String (a configured tag group) |
| Deadband Mode (history) | `historicalDeadbandMode` | `Absolute` \| `Percent` \| `Off` |
| Historical Deadband | `historicalDeadband` | Numeric |

(05-06 used `historicalProvider` — absent from the official table. This is hypothesis-with-strong-evidence, not a substitute for the diff.)

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|---|---|---|---|
| Resource family targeted `/projects/{p}/resources/**` REST | Export/import zip surgery (`client/resources.rs`) | 05-02 (v1.0) | The ONLY real wire; workspace/edit build on it |
| Gateway↔gateway diff only | MemberSource generalization (zip ∪ fs-tree sources) | THIS PHASE | Local dir becomes a first-class authoring surface |
| `--decode-scripts` on `ign project export` only | Same codec reused by checkout AND `ign edit` | THIS PHASE (codec complete since 07-04) | No codec work left; only orchestration + hardening |
| Tag↔historian = "documented limitation" with 5 failed candidates (05-06) | Doc-corrected property vocabulary (`historyProvider` vs `historicalProvider`) + mandated Designer-diff spike | THIS RESEARCH (2026-09-14) | Spike starts with a leading hypothesis; TAGS-14 closure is plausibly achievable on the tag write path |
| OutOfBand set `["completions", "api call"]` | + `edit` (pre-declared reservation from 08-06 lands) | THIS PHASE | Pinned test extends in the same task as the clap command |

**Deprecated/outdated:** none in this phase's path — `historicalProvider` as a tag-config key name is the (05-06-era) artifact to retire in favor of whatever the Designer-diff captures.

## Open Questions

1. **Does the 05-06 null-data outcome reduce to the `historicalProvider` typo?**
   - What we know: official 8.3 property table says `historyProvider`; 05-06 sent `historicalProvider`; symptoms (silent accept, no data) match an ignored key; sample-mode/historical-group settings were also never sent.
   - What's unclear: whether the tagConfig route/gateway silently drops unknown keys vs stores them as unknown properties (the BEFORE/AFTER diff in the spike answers this as a side effect); whether the Historical-Tag-Group key name is `historicalScanclass`.
   - Recommendation: plan 01 captures the Designer diff FIRST (oracle), then tests the replay using only diff-captured names. Do not pre-commit `historicalScanclass` anywhere pinned.
2. **FS-path escaping scheme for hostile member names.**
   - What we know: zip-side mapping (`user_path`/`member_path`) is proven; the fs side is new; APFS case-insensitivity is the real collision surface.
   - What's unclear: percent-encoding vs refusal-only for names needing escape; which characters are legal in Ignition member names in practice (spike-adjacent evidence exists in exports but no corpus was captured).
   - Recommendation: plan-level decision — escape scheme pinned by proptest bijection + round-trip property tests; collisions that escaping cannot resolve (case-fold) refuse at checkout with BOTH paths named.
3. **Where `workspace` sits in the clap tree** (`ign workspace checkout|status|push` per REQUIREMENTS IDE-04 — three verbs, one family) and the exact envelope shapes for status/push.
   - Recommendation: planner's discretion; follow the project-diff/sync result-shape conventions (flat agent shape, ALL keys always, B-relative semantics reconciled EXPLICITLY).
4. **Whether the Designer writes binding state outside the tag config** (Pitfall 12's secondary trap: provider/module-level settings).
   - What we know: the docs route binding through tag properties; provider-level history settings exist separately (Store-and-Forward etc.).
   - Recommendation: the spike's diff must capture tag config AND, if the tag-level replay fails, a `config`/provider-config export probe before declaring the limitation — the plan should include that probe as a bounded step.

## Sources

### Primary (HIGH confidence)
- Codebase (all file:line refs verified 2026-09-14): `crates/ignition-core/src/client/resources.rs`, `crates/ignition-core/src/client/scripts_codec.rs`, `crates/ignition-core/src/actions/projects.rs`, `crates/ignition-core/src/actions/resources.rs`, `crates/ignition-core/src/actions/tags.rs`, `crates/ignition-core/src/actions/lint.rs`, `crates/ignition-cli/tests/e2e_webdev.rs`, `crates/ignition-cli/tests/tui_coverage.rs`
- Context7 `/websites/inductiveautomation_8_3` — Tag Properties (History Properties table: `historyEnabled`, `historyProvider`, `sampleMode`, `historicalDeadbandMode`, `historicalDeadband`); Configuring Tag History (enable steps: enabled + storage provider + Sample Mode Tag Group + Historical Tag Group; dataset tags unsupported); Exported Tags XML (live-doc shape showing `historyProvider` + `historyEnabled`)
- `.planning/phases/05-webdev-backend-tag-operations/05-06-PLAN.md` + `05-06-SUMMARY.md` — the v1.0 spike: candidates, outcome, live-run discipline, byte-0 contract, Designer-diff resolution path
- `.planning/research/STACK.md`, `ARCHITECTURE.md`, `PITFALLS.md` (v1.1 research; pitfalls 12/15/17 are the phase's pitfall backbone; ARCHITECTURE P7-P9 sketches confirmed against code)
- `.planning/phases/11-tag-bulk-transfer-xml-csv/11-RIG-NOTES.md` + `11-LIVE-CAPTURES.md` — rig procedures, unique-provider names, trial-window ops

### Secondary (MEDIUM confidence)
- `.planning/STATE.md` decisions ledger (Phase 8/10/11/12 contracts: Three-Place, OutOfBand, guard ladder, route-bump atomicity, trial ops)
- README.md contract sections (envelope/exit-codes/OutOfBand/decode-scripts) — additive-only constraints

### Tertiary (LOW confidence — spike validates)
- `historicalScanclass` as the Historical-Tag-Group JSON key (training-data recollection; NOT in the captured doc table rows — the Designer diff names it)
- Whether the gateway silently drops vs stores unknown tag-config keys (the spike's before/after diff answers it)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new runtime deps; every component is in-tree and phase-proven; the single dev-dep (`proptest`) is an additive, binary-invisible decision
- Architecture (workspace/edit): HIGH — the MemberSource refactor scope was verified line-by-line against the pure engine; edit's pipeline composes existing, contract-tested pieces
- Architecture (historian): MEDIUM — the write path exists, but the binding's wire shape is the spike's to capture (by roadmap design); the doc-based correction raises closure odds materially
- Pitfalls: HIGH — pitfalls 12/15/17 were pre-written for exactly this phase in v1.1 research and re-verified against the current code; the trial-window and contract-ritual pitfalls carry four phases of live ops evidence

**Research date:** 2026-09-14
**Valid until:** spike outcome supersedes §Historian immediately after plan 01; the rest is stable (codebase-anchored) for the phase's duration.
