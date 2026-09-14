# Debug Session: xml_udt_type_definition loss-fact text falsified live (UAT Phase 11, test 4)

- **Date:** 2026-09-14
- **Mode:** find_root_cause_only (research + bounded read-only characterization; no source/planning-doc changes)
- **Discovered:** UAT 2026-09-14 against live 8.3.6 rig, localhost:18188, profile `uat`
- **Repro:** Tests 3/4 in `.planning/phases/11-tag-bulk-transfer-xml-csv/11-UAT.md`; fixture `.planning/phases/11-tag-bulk-transfer-xml-csv/artifacts/udt-multilevel.xml`, `ign tags import --format xml --yes`

## Symptoms (verbatim from UAT)

The fact text claims:

> file contains UDT type definition(s) (type="UdtType") — importTags refuses them ("Udt definitions can only be imported in the UDT Definitions tab"), so the import lands nothing

Live test with `--yes`: the import FULLY LANDED — envelope `imported: 2, failed: []`; rig verification showed `MotorType` WITH member children (Amps, Doubled) under `[default]_types_`, and `P11UDT` WITH children (M1, M2, Sub Folder) at provider root. Tests 3 (gate refusal, pre-resolution) and 5 (byte-identical round-trip) behaved correctly — the ONLY defect is the fact TEXT's live-behavior claim.

## Capture-claim analysis — what 11-01 actually proved

The verbatim refusal is real and was captured on both rigs. 11-LIVE-CAPTURES.md §Probe 3, determinism finding (b)(1):

> **`importTags` REFUSES UDT type definitions** (verbatim, BOTH rigs): `Error_Exception("Error importing tags: Udt definitions can only be imported in the UDT Definitions tab.")` — the import lands NOTHING; the subsequent re-export of the target paths yields empty `type="Unknown"` shells (154 bytes, verbatim in probe-outputs). Type-definition-bearing XML files CANNOT round-trip through `importTags` at all.

**The condition under which that refusal was observed:** the 11-01 probe-3(b) import target was `[default]P11Roundtrip` — a FOLDER basePath, not provider root. Evidence: 11-01-PLAN.md task text ("Then import the XML into `[default]P11Roundtrip` via the probe-2 importTags pattern ('o' policy) and re-export") and 11-LIVE-CAPTURES.md (b)(2), which names `[default]P11Roundtrip` explicitly for the instance-only sibling of the same attempt. The 11-01 rigs are gone (raw probe outputs wiped with `/tmp/ign-p11-rigs`, per 11-06-SUMMARY), so the capture cannot be re-inspected — but plan text + captures doc agree on the folder target.

Everything ELSE in the capture matches the UAT run exactly:

- File bytes: IDENTICAL — sha256 `11ed152868795fc8451067795176fbf72b5e039a77a8752cbe042bb8e4416c36` verified on both committed copies (artifacts/ and crates/ignition-core/tests/fixtures/).
- Gateway version: 8.3.6 in both.
- Collision policy: `'o'` (capture) == `--yes` overwrite (UAT).
- Call chain: capture = scriptExec `system.tag.importTags(tmpPath, basePath, 'o')`; CLI route = `importTagsFile` writes a gateway temp file and calls the same `system.tag.importTags` (11-02-PLAN §B). Same gateway entry point.
- Temp-file suffix: `.xml` in both (probe 2's `createTempFile('ign-probe','.xml')`; route 1.3.0 suffixes by format — the '.tagimport' suffix bug was a different, fixed delta).

The single remaining variable is the **basePath class**: folder (`[default]P11Roundtrip`) in the capture vs provider ROOT (`[{provider}]`) in the CLI. The CLI hard-codes provider root — `crates/ignition-core/src/actions/tags.rs:2210`: `"basePath": format!("[{provider}]")` in `tags_import_bulk`.

**How the overgeneralization propagated:** 11-01-SUMMARY ("importTags REFUSES UDT type definitions… UdtType-bearing XML cannot round-trip through importTags at all") → STATE.md:134/143 → 11-03's fact detail (tag_loss.rs:172-180) written as UNCONDITIONAL → README.md §"The loss gate (imports, TAGS-12)" (~:1118, "UDT type definitions the gateway refuses outright"). Note the 11-06 fidelity gate quietly worked AROUND the claim (config-creates `MotorType` at the target; its seeded XML is "instance-only by construction", 11-LIVE-GATE.md SC-1 row; delta 4 note "the importTags UdtType REFUSAL applies to import-file XML, not the config surface") — the gate never re-tested the refusal itself, so the stale text survived into UAT.

## Live characterization (disposable rig, provider `uatcsv` ONLY, 3 probes, 2026-09-14)

CLI: the current cargo-cache build (`/Users/pmannion/Library/Caches/cargo-target/debug/ign`, Sep 14 04:54, route 1.3.0 — the stale repo-local `target/debug/ign` expects route 1.1.0 and was NOT used). All imports: `--collision-policy overwrite --yes` (matches UAT). basePath is always provider root (the only form the CLI exposes — a folder-basePath probe needs the scriptExec secret, which is not staged in `/tmp/ign-uat/tokens.env`, so the folder-vs-root boundary below rests on capture provenance; the provider-root landings are live-proven).

**Probe 1 — UdtType-ONLY file (no instance), target type absent.** Crafted `/tmp/.../udt-type-only.xml` (`ProbeType` UdtType, parameter `ProbeNum`, one AtomicTag child `ProbeChild`) → import into `uatcsv`:

- Envelope: `ok:true, imported:1, failed:[]` — the fact text says "lands nothing"; it landed everything.
- `tags browse '[uatcsv]_types_'` → `ProbeType` `tag_type:"UdtType"`, `has_children:true`; child browse shows `ProbeChild` AtomicTag.
- KEY SHAPE FINDING: the definition was ROUTED OUT of the import basePath into `[provider]_types_/Name` — same as the UAT `MotorType` observation. Provider-root imports place definitions in `_types_` regardless of their position in the file.

**Probe 2 — full mixed fixture (type + instances), target type ABSENT.** The byte-identical `udt-multilevel.xml` (sha `11ed1528…`) into `uatcsv`:

- Envelope: `imported:2, failed:[]`, loss_facts carried both advisory codes (gate/gate-text unchanged — only the text is wrong).
- `tags browse '[uatcsv]'` → `P11UDT` Folder with `M1`/`M2` UdtInstances + `Sub` Folder at provider root; `tags browse '[uatcsv]_types_'` → `MotorType` UdtType present. `_types_` held only `UatSeed` before the import, so "type must pre-exist in target" is excluded as the refusal condition.

**Probe 3 — re-import with the type ALREADY PRESENT.** Same fixture re-imported (MotorType now in `uatcsv` from probe 2): `imported:2, failed:[]`; member browse shows `Amps`/`Doubled` — "type already exists in target" is excluded too; overwrite converges.

Probes left `uatcsv` residue (`ProbeType`, the fixture subtree) — disposable scratch, intentional.

## Root cause

`codes::XML_UDT_TYPE_DEFINITION` and its fact detail (crates/ignition-core/src/actions/tag_loss.rs:27-30 and :172-180) state an UNCONDITIONAL gateway refusal — "importTags refuses them … so the import lands nothing" — for ANY file containing `type="UdtType"`. That claim is a **scope error**: it generalizes the 11-01 probe-3(b) capture, which was taken with a FOLDER basePath (`[default]P11Roundtrip`), into a blanket fact. The shipped CLI import path (`tags_import_bulk`) always sends the PROVIDER-ROOT basePath (`"[{provider}]"`, tags.rs:2210), and at provider root the gateway ACCEPTS UdtType definitions — routing each definition to `[provider]_types_/Name` and landing everything else under the basePath (live-proven 3× on 8.3.6: type-only, mixed with target type absent, re-import with target type present; and in UAT itself). So on the only path the CLI exposes, the text is ALWAYS wrong: the import lands, and the definitions land under `_types_`. The verbatim refusal remains true for folder-basePath imports on both gateway generations — a condition the CLI cannot currently express.

## Files involved

- `crates/ignition-core/src/actions/tag_loss.rs:172-180` — the fact `detail` string: the falsified claim (THE defect).
- `crates/ignition-core/src/actions/tag_loss.rs:27-30` — `codes::XML_UDT_TYPE_DEFINITION` doc comment repeats it.
- `crates/ignition-core/src/actions/tag_loss.rs:439-441` — test comment ("importTags REFUSES type definitions and lands nothing"); the `has_fact` assertion itself stays valid (fact presence is correct; only the text lies).
- `README.md` ~line 1118 (§"The loss gate (imports, TAGS-12)") — "UDT type definitions the gateway refuses outright".
- `.planning/STATE.md` line 134 (11-01 decision) and line 143 (11-03 decision) — repeat the unscoped claim.
- `.planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md` §Probe 3 determinism (b)(1) + oracle item 4 — capture text itself is accurate for its conditions but lacks a scope marker; needs a dated correction note (doc has precedent: §Probe-5 finding 1, corrected 2026-09-14 in commit `361d6b4`).
- Related-but-separate truth the fact text should absorb: 11-06 delta 4 — instance parameter overrides silently drop when the referenced type is unresolvable in the TARGET provider (8.3.3; 8.3.6 preserves), and exported `udtParentType` is provider-qualified (`[default]_types_/X`), so cross-provider imports keep pointing at the SOURCE provider.

## Suggested fix direction (for plan-phase --gaps)

1. Reword the `detail` text (and the code doc comment) to the true, conditioned behavior — required truth elements: (a) definitions DO import via this command; they land under `[provider]_types_/Name` regardless of position in the file; (b) the verbatim refusal "Udt definitions can only be imported in the UDT Definitions tab" is a folder-basePath behavior (11-01 capture) that this command's provider-root import does not hit; (c) instances' `udtParentType` is provider-qualified in exports — cross-provider imports keep pointing at the source provider, and parameter overrides silently drop when the type is unresolvable in the target provider (8.3.3). Consider keeping the fact UNCONDITIONAL in firing (detection is correct and useful — the transfer ISN'T lossless: definition routing + reference rewriting caveats are real losses worth warning about) but fix WHAT it claims.
2. Mirror the correction in README §"The loss gate (imports, TAGS-12)" and STATE.md lines 134/143 (dated correction notes per house precedent).
3. Add the dated scope note to 11-LIVE-CAPTURES.md §Probe 3 (b)(1)/oracle item 4 rather than editing history (Probe-5 precedent).
4. Update the tag_loss.rs:439-441 test comment; check 11-05 report goldens for any pin on the detail string (grep found none outside tag_loss.rs itself — `lands nothing` occurs only there in source/tests besides unrelated CSV asserts in tags_contract.rs).
5. Optional follow-up (separate decision): expose a folder/basePath override for XML imports if folder-scoped imports are ever wanted — the verbatim refusal is real there and the CLI currently cannot reach that condition at all.
