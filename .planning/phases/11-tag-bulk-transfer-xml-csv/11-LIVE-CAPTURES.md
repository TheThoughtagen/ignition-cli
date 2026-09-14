# Phase 11 — Live Gateway Captures (11-01)

**Purpose:** the five roadmap-mandated live-gateway probe answers (plus the legacy CSV coverage table), captured verbatim from two disposable rigs (see 11-RIG-NOTES.md). Everything downstream (route design, scan design, CSV generation, fidelity oracle) cites THIS document instead of re-deriving wire truth — the Phase-10 capture-first discipline. **No secret material in this file.**

**Provenance:** all outputs below are verbatim responses from real gateways — Rig A `inductiveautomation/ignition:8.3.6` (container `ign-p11-836`, port 18188) and Rig B `inductiveautomation/ignition:8.3.3` (compose `ign-p11-833-ignition-1`, port 19188), both commissioned 2026-09-11, both running the deployed 1.1.0 WebDev bundle. Raw JSON responses also staged outside the repo at `/tmp/ign-p11-rigs/probe-outputs/`.

## Method (probe vehicle)

- Vehicle: `POST /system/webdev/ign-cli/cli/scriptExec` with JSON body `{"action":"exec","code":<Jython>}` + the `x-ignition-cli-secret` header (deployed secret; value never in repo). The `version` handshake (`routeVersion 1.1.0`) was confirmed on both rigs before probing.
- Code shape: STATEMENT form (the 05-06 Jython rule) with the surfaced value assigned to `_result`; per-sub-probe `try/except` with `traceback.format_exc()` for verbatim exception capture.
- **Live-truth finding (recorded, affects all later scriptExec use):** the exec action's code runs in a FRESH globals dict — the WebDev module scope (which carries `system`) is NOT inherited. First probe attempt failed with `NameError: name 'system' is not defined` on both rigs (verbatim traceback in §Method-finding below). Bridge: an explicit `import system` line at the top of the code resolves the full Ignition scripting surface from the exec scope on both rigs. Prior live gates only ever exercised the `version` action, so this is the first live proof of the exec action's scope semantics.
- Caveat honored (planner lock): scriptExec runs on a script thread, NOT a WebDev thread. All findings below are **script-thread truth**; the 11-06 live gate exercises the true WebDev-thread path through the real route actions.

### Method-finding: exec scope does not carry `system` (first attempt, verbatim, both rigs)

```
{"error":{"code":"route_error","message":"scriptExec route error","traceback":"Traceback (most recent call last):\n  File \"<<ign-cli/cli/scriptExec:doPost>>\", line 171, in doPost\n  File \"<string>\", line 1, in <module>\nNameError: name 'system' is not defined\n"},"ok":false}
```

Fix: `import system` as the first code line. With it, `system.tag.*` resolves identically on 8.3.6 and 8.3.3 (`system.tag.browse('[default]', {})` returns a live `Results$PyWrapper`).

### Seed

`system.tag.configure('[default]', [{P11Seed Folder with T1 memory=42 / T2 expression '1+1'}], 'a')` → `['Good']` on both rigs (repr-verbatim: `seed configure results: ['Good']`).

---

## Probe 1 — exportTags kwargs+xml return form (research Open Question 2 / Pitfall 1)

**Question:** does the kwargs return-string form accept `exportType='xml'`?

**Answer: YES — returns the full XML document as a `unicode` string. Both rigs.** No temp-file fallback needed for XML export (the route still carries one for import). Details:

| Sub-probe | Form | Result |
| --- | --- | --- |
| 1a | `exportTags(tagPaths=['[default]P11Seed'])` | `unicode`, len 324 — JSON interchange, `{\n` (LF) pretty-printed, keys unordered |
| 1b | `exportTags(tagPaths=['[default]P11Seed'], exportType='xml')` | `unicode`, len 537 — **full XML document** (see verbatim below) |
| 1c | `exportTags('[default]P11Seed', ['[default]P11Seed'], True, 'xml')` (documented positional form) | `unicode`, len 40 — **arg-1 is interpreted as filePath**: returns `/usr/local/bin/ignition/[default]P11Seed` (gateway cwd-relative path where the export was WRITTEN). No error. |

### 1b verbatim (both rigs byte-identical, repr shown for the CRLF evidence)

```
'<Tags MinVersion="8.0.0" locale="en_US">\r\n   <Tag name="P11Seed" type="Folder">\r\n      <Tags>\r\n         <Tag name="T2" type="AtomicTag">\r\n            <Property name="valueSource">expression</Property>\r\n            <Property name="expression">1+1</Property>\r\n         </Tag>\r\n         <Tag name="T1" type="AtomicTag">\r\n            <Property name="valueSource">memory</Property>\r\n            <Property name="defaultValue">42</Property>\r\n            <Property name="value">42</Property>\r\n         </Tag>\r\n      </Tags>\r\n   </Tag>\r\n</Tags>\r\n'
```

(Rig B returns the same 537 bytes with `T1`/`T2` blocks swapped — child ORDER differs per rig/map iteration, see §Byte-shape findings.)

### Probe-1 answers

1. **Return form:** kwargs+xml returns a STRING (type `unicode`), same as the JSON kwargs form. The Phase-5 "kwargs-only return-string" rule extends to `exportType='xml'`.
2. **XML declaration:** **NONE.** The document starts directly at `<Tags MinVersion="8.0.0" locale="en_US">` — no `<?xml version="1.0"?>` prolog. (The official-docs sample implies a declaration; the gateway does not emit one.)
3. **Full document vs fragment:** full document — single `<Tags>` root wrapping the exported subtree, root carries `MinVersion` + `locale` attrs.
4. **Line endings: CRLF (`\r\n`) throughout, 3-space indentation, and a trailing `\r\n` after `</Tags>`.** (The JSON interchange uses LF and has NO trailing newline.) Byte-fidelity handling (base64 envelope) is therefore mandatory — any text-normalizing transport would corrupt the oracle.
5. **Positional form:** live-proven as the filePath-writing fallback (writes bytes to the given path, returns the path string). The gateway cwd is `/usr/local/bin/ignition` — a relative path lands there. Probe-1c side effect: the file `/usr/local/bin/ignition/[default]P11Seed` exists inside each disposable container (cleaned up at teardown by container removal; noted for ops honesty).
6. **Cross-rig delta:** ZERO byte-level header/declaration/indentation delta between 8.3.3 and 8.3.6 — only child-element ORDER differs (map-iteration order, also visible in the JSON baseline).

---

## Probe 2 — importTags from a script thread (research Open Question 3)

**Questions:** accepted basePath forms (provider root vs subfolder), QualityCode return shape, temp-file writability, collision semantics.

**Answers (script-thread truth, both rigs identical in behavior):**

| Sub-probe | Call | Result |
| --- | --- | --- |
| 2a | `importTags(tmpPath, '[default]P11Import', 'a')` | **Succeeds.** Returns `ArrayList[QualityCode]` — `str()` per element: `['Good','Good','Good']` (folder + T1 + T2). Import lands the subtree under the basePath: `[default]P11Import/P11Seed/…` |
| 2b | `importTags(tmpPath, '[default]', 'a')` — provider ROOT | **NO No-RpcContext error.** Returns `ArrayList[QualityCode]`: `['Good', Bad_Failure("Tag '[default]P11Seed/T2' already exists, and 'abort' collision policy has been specified"), Bad_Failure("Tag '[default]P11Seed/T1' already exists, …")]` — the 07-06 RpcContext constraint does NOT apply to importTags (it applies to getConfiguration/exportTags). Folders merge at collision; only leaf tags hit the collision policy |
| 2c | same file again, policy `'o'` | **Succeeds** — `['Good','Good','Good']` (full overwrite) |
| 2d | same file again, policy `'a'` | **NOT a thrown exception.** Returns QualityCodes with per-element failures: `['Good', Bad_Failure("Tag '[default]P11Import/P11Seed/T2' already exists, and 'abort' collision policy has been specified"), Bad_Failure(…T1…)]` |

### Temp-file facts (both rigs)

- `java.io.File.createTempFile('ign-probe','.xml')` → `/tmp/ign-probe<n>.xml` (`java.io.tmpdir` = `/tmp`), writable, 537 bytes.
- try/finally delete → `deleted: true`, zero `ign-probe*` residual files after the probe (temp-dir listing checked).

### Probe-2 answers (route-design implications)

1. **basePath forms:** BOTH `[provider]folder` and `[provider]` (provider root) accepted from a script thread. The provider-root RpcContext refusal (07-06) is getConfiguration/exportTags-specific — importTags is free of it.
2. **QualityCode shape:** `ArrayList` of `QualityCode` objects; `str(x)` renders `Good` / `Bad_Failure("…")`. The route's `[str(x) for x in results]` translation (the configure action's existing pattern) is correct as-is.
3. **Collision semantics — plan hypothesis CORRECTED:** policy `'a'` on collision does NOT raise; the import returns normally with `Bad_Failure("Tag '<path>' already exists, and 'abort' collision policy has been specified")` elements. The route/action layer must INSPECT the QualityCode list for `Bad_Failure`/`Error_*` elements rather than catching exceptions. `'o'` overwrites cleanly.
4. **Folder-merge rule:** import does not fail on a colliding FOLDER — folders merge; leaf tags carry the collision.
5. **JVM temp writability:** proven (both rigs, `/tmp`).

---

## Probe 4 — mixed-parent exportTags (research Open Question 5 / Pitfall 2)

**Question:** hard error / silent partial / works — and is the constraint new-format-only?

**Answer: SILENT CORRUPTION, not an error — and it is PRE-EXISTING in the JSON interchange too. Both rigs identical.**

| Sub-probe | Paths | Result |
| --- | --- | --- |
| 4a same-parent XML | `['[default]P11Seed/T1','[default]P11Seed/T2']` | Clean 404-byte XML doc, both tags full (verbatim in probe-outputs) |
| 4b mixed-parent XML | `['[default]P11Seed/T1','[default]P11Import/T1']` | 294-byte XML; the foreign-parent tag exports as an EMPTY skeleton: `<Tag name="T1" type="Unknown">\r\n   </Tag>` — no error raised |
| 4c mixed-parent JSON (no exportType) | same paths | `{"tags":[{"defaultValue":42,…,"tagType":"AtomicTag"},{"name":"T1","tagType":"Unknown"}]}` — SAME corruption in the JSON baseline |

### 4b verbatim (both rigs byte-identical)

```
'<Tags MinVersion="8.0.0" locale="en_US">\r\n   <Tag name="T1" type="AtomicTag">\r\n      <Property name="valueSource">memory</Property>\r\n      <Property name="defaultValue">42</Property>\r\n      <Property name="value">42</Property>\r\n   </Tag>\r\n   <Tag name="T1" type="Unknown">\r\n   </Tag>\r\n</Tags>\r\n'
```

### Probe-4 answers

1. **Behavior class:** silent partial/corruption (`type="Unknown"` empty Tag element) — NOT a hard error, NOT a clean drop. The official "all tag paths must be from the same parent folder" constraint is enforced by OUTPUT MUTILATION, not by an exception.
2. **New-format-only?** NO — the JSON interchange (the CLI's existing export format) exhibits the identical corruption (`"tagType":"Unknown"`). The mixed-parent pre-check the loss report / route needs is therefore a BOTH-formats concern, not an XML-only guard. (Whether existing JSON CLI behavior gains the pre-resolution refusal is a later-plan decision — this capture establishes it is pre-existing gateway behavior, so gating it does not "change" gateway semantics.)
3. The loss-scan's `type` tally will naturally surface `Unknown` tags — the corruption shape is detectable in a scanned FILE as well as pre-resolution.

---

## Byte-shape findings so far (cross-rig, feeds the fidelity-oracle design)

- XML: CRLF + 3-space indent + trailing CRLF + NO XML declaration — identical constants on 8.3.3 and 8.3.6.
- JSON: LF + 2-space indent, NO trailing newline — identical constants on both.
- Child ORDER within a folder/JSON object differs per rig (and per call, map-iteration order). **Any byte-identity oracle must therefore compare same-rig exports only** — cross-rig byte comparison is invalid by construction.
- Probe-1c artifact: the positional form wrote `/usr/local/bin/ignition/[default]P11Seed` inside each container (removed with the containers at teardown).

---

## Probe 3 — REAL multi-level UDT XML byte shape + determinism (research Open Question 4 — the roadmap's explicit flag)

**Construction (CLI-realistic, via the deployed tagConfig route, not scriptExec):** `MotorType` UDT definition created with `ign tags config create '[default]_types_/MotorType' --file def.json` then `tags config edit` to fix shapes; instances `M1`/`M2` + nested folder `Sub`/`M3` via `tags config create '[default]P11UDT'`. Definition: parameter `MotorNumber` (Int4, value 1), child `Amps` (OPC, opcItemPath parameter-bound, alarm `Low Amps` priority High setpointA 25), child `Doubled` (expression, parameter-bound).

**Construction-shape findings (def JSON round-trip, both rigs):** parameter key must be `value` (a `defaultValue` key is silently dropped — read-back showed `{datatype=Integer, value=null}` until fixed); alarm `mode` key does not bind (stays null, exports as EMPTY `<Property name="mode"/>`); alarm `priority` accepts the string enum (`"High"`) and renders numerically (`3`) in XML.

### The multi-level export (both subtrees in one clean call)

Single-path export of `[default]P11UDT` = 1128 bytes: **instances export as REFERENCES only** — `udtParentType` + own `Parameters`, NO expanded UDT children, NO alarms (the "edited properties only" rule; children live in the type). The CompoundProperty alarms block lives in the TYPE definition, so the artifact capture is a **two-path export** `exportTags(tagPaths=['[default]P11UDT','[default]_types_/MotorType'], exportType='xml')` — which merges cleanly (2218 bytes, NO Unknown corruption: the probe-4 corruption hits leaf-tag sibling lists, not folder-level nodes from different parents).

**Artifact:** `artifacts/udt-multilevel.xml` = the EXACT returned bytes (Rig A, sha256 `11ed152868795fc8451067795176fbf72b5e039a77a8752cbe042bb8e4416c36`, 2218 bytes, never re-indented or edited). Checklist vs actual:

| Expected element | In the capture? |
| --- | --- |
| Root `<Tags MinVersion="8.0.0" locale="en_US">` | YES |
| `type="UdtType"` definition | YES (`MotorType` with expanded children) |
| `type="UdtInstance"` instance markers | YES (`M1`/`M2`/`M3`) |
| Nested multi-level Tag elements (folder → instance, folder → folder → instance) | YES |
| Parameter encoding | YES: `<Parameters><Property name="MotorNumber" type="Integer">2</Property></Parameters>` |
| **CompoundProperty alarms block** | YES: `<CompoundProperty name="alarms"><PropertySet><Property name="setpointA">25</Property><Property name="mode"/><Property name="name">Low Amps</Property><Property name="priority">3</Property></PropertySet></CompoundProperty>` |
| Parameter-bound property encoding | YES: `<Property name="opcItemPath" boundValueType="parameter">ns=1;s=[Dairy]Motor {MotorNumber}/Amps</Property>` (matches the official-docs sample shape exactly) |
| XML declaration | **NO** (none anywhere — consistent with Probe 1) |

JSON interchange cross-reference (trimmed, same two-path scope, Rig A): instances appear as `{"name":"M2","parameters":{"MotorNumber":{"dataType":"Integer","value":2}},"tagType":"UdtInstance","udtParentType":"[default]_types_/MotorType"}`; the type as `"tagType":"UdtType"` with nested `tags` (children) — sibling order M2/Sub/M1 differs from the XML's same-run order (independent map iterations).

### Determinism answers (the fidelity-oracle decision)

**(a) Unchanged-subtree export determinism: BYTE-IDENTICAL.** Three consecutive exports of the unchanged subtree are byte-equal (`e1==e2`, `e2==e3`, `e1==e3` all true) on BOTH rigs (2218 bytes each). The map order is stable within a gateway process for unchanged state.

**(b) Import→re-export byte-identity: NOT ACHIEVABLE — two distinct reasons, both captured:**

1. **`importTags` REFUSES UDT type definitions** (verbatim, BOTH rigs): `Error_Exception("Error importing tags: Udt definitions can only be imported in the UDT Definitions tab.")` — the import lands NOTHING; the subsequent re-export of the target paths yields empty `type="Unknown"` shells (154 bytes, verbatim in probe-outputs). Type-definition-bearing XML files CANNOT round-trip through `importTags` at all.
2. **Instance-only XML round-trips structurally but NOT byte-identically:** importing the P11UDT-only export into `[default]P11Roundtrip` (`'o'`) succeeds (`Good` ×5) and re-exports at the IDENTICAL length (1128 == 1128) with all elements/attrs/properties equal — but sibling order permutes on the model rebuild (first divergence at byte offset 115: original `M2,Sub,M1` vs re-export `M1,Sub,M2`). Order-normalized comparison (recursive sibling sort + canonical serialize) = **IDENTICAL**.

**NAMED ORACLE STRATEGY (binds 11-06's live gate):**

1. Transport fidelity (always): `sha256(file bytes) == sha256(base64decode(payload_b64))`.
2. Export determinism (proven): re-export of an UNCHANGED subtree is byte-stable → byte-identity valid for same-subtree re-export without intervening writes.
3. Import→re-export oracle: byte-identity is INVALID (order permutes on rebuild). Use **order-normalized structural identity** (canonicalize: recursively sort sibling elements, compare serialization) + length equality as a cheap pre-check. Captured pair: orig sha `dcf1c0aab952f287…` vs re-export sha `2d99ed58c2fc4dda…` (equal length, structurally identical).
4. Files containing `type="UdtType"` are EXCLUDED from the round-trip oracle — the gateway refuses their import (finding b.1); the loss-scan must surface UdtType presence and the gate may only assert the verbatim refusal element for them.

### Cross-rig byte delta (8.3.6 vs 8.3.3)

Same construction repeated on Rig B: export is 2218 bytes, 3× byte-deterministic, and the UdtType import refusal is the identical verbatim string. Rig A sha `11ed1528…` vs Rig B sha `46da5295…` — **the entire delta is sibling order** (the `M1` block occupies a different position; 612 differing bytes, zero header/declaration/indentation/encoding differences). Cross-rig byte comparison of exports is invalid by construction; same-rig same-state comparison is valid.

---

## Probe 5 — Legacy CSV coverage map (research Open Question 6)

**Method note:** the probe CSVs were generated by hand-rolled Jython row builders; two of the failures recorded below (comma-in-`FormatString` column shift; the hunt that followed) are LIVE PROOFS of the research's warning that hand-rolled quoting is the trap the `csv` crate exists to prevent. A Java-side ground-truth check (`TagCSVImporter` + `PROP_COLUMNS` static field read from `common.jar` inside the 8.3.6 container, identical constant on 8.3.3) anchors the vocabulary answer.

### Header grammar (authoritative)

- Structural columns: `Path,Name,Owner,TagType,DataType` (lowercased name-matching — `headers` map keys are `toLowerCase()`d).
- Property columns = `TagCSVImporter.PROP_COLUMNS`: **exactly 51 names, identical on both rigs**: Value, Enabled, AccessRights, OPCServer, OPCItemPath, ScanClass, DriverName, ScaleMode, RawLow, RawHigh, ScaledLow, ScaledHigh, ClampMode, ScaleFactor, Deadband, DeadbandMode, FormatString, EngUnit, EngLow, EngHigh, EngLimitMode, Tooltip, Documentation, ExpressionType, Expression, AlertMode, AlertAckMode, AlertSendClear, AlertMessageMode, AlertMessageSubject, AlertMessage, AlertDeadband, AlertTimestampSource, AlertNotes, AlertDisplayPath, OPCWriteBackServer, OPCWriteBackItemPath, SQLBindingDatasource, HistoryEnabled, PrimaryHistoryProvider, HistoricalScanclass, HistoricalDeadband, HistoricalDeadbandMode, InterpolationMode, HistoryMaxAgeMode, HistoryMaxAge, HistoryTimestampSource, UDTParentType, PersistValue, SourceDataType, SourceTagPath.
- vs the official docs' table: the docs' `SQLBindingPollRate` + `Permissions` are **NOT** in PROP_COLUMNS; nine legacy `Alert*` columns exist that the docs table omits. The docs' 47/48-column sample header is therefore neither the parser's vocabulary nor required — ANY header subset of the grammar works (11-col docs sample ✓, 12-col ✓, full 56-col ✓).
- Two SPECIAL headers outside PROP_COLUMNS are parsed by dedicated code paths: **`Permissions`** and **`AlarmStates`** (bytecode-extracted formats below).
- Unknown header columns (e.g. modern `TagGroup`, or `NameX`) are **silently ignored** (import Good, column absent from the landed model) — provided every numeric-required cell present. Row-level failures are wholesale: one bad row aborts the import and NOTHING lands (all-or-nothing).
- Marker row `# version=N` REQUIRED (missing → `Unknown CSV Format`); `N` bounds-checked (1 and 2 accepted; 99 → verbatim `Error_Exception("Error importing tags: Incompatible CSV format (99)")`); version is NOT coupled to header width (version=2 works with the 11-col header).

### The Path/Owner findings (the plan's folder-row assumption, corrected)

- **ANY non-empty `Path` cell NPEs the importer** (verbatim, both rigs): `Error_Exception("Cannot invoke \"String.equals(Object)\" because \"name\" is null")` — the folder-branch of `importInternal` is broken on the script-callable path. Folder structure is INEXPRESSIBLE; only rows with an EMPTY Path import.
- **The `basePath` argument is IGNORED for CSV imports:** tags land at the PROVIDER ROOT regardless (proven by importing with a pre-created target folder `[default]P11HdrC` — read-back empty, tag landed at `[default]TC1`). This differs from XML imports, which DO land under the basePath (probe 2).
- **`Owner` non-empty is UNUSABLE:** the value is treated as an import target path → `Bad_Unsupported("The target path '[default]covowner' cannot accept children tags.")`.

### Column-by-column coverage table (the documented lossy-field table 11-04/11-05 cite)

Legend: LANDED = value reaches the modern model (JSON key noted); COERCED = lands but transformed; DROPPED = accepted but absent from the model; UNUSABLE = crashes/aborts; UNTESTED = no positive evidence this run.

| Column | Verdict | Landed as / evidence |
| --- | --- | --- |
| Path | **UNUSABLE** | any non-empty value → importer NPE (above) |
| Name | LANDED | `name` |
| Owner | **UNUSABLE** | non-empty → `Bad_Unsupported` abort |
| TagType | COERCED | numeric → `tagType` + valueSource class: 0→`opc`, 1→`memory` (default), 6→`Folder`, 10→`UdtInstance`; 13 (Derived) untested |
| DataType | COERCED | numeric → `dataType` name: `2`→`Int4`, `7`→`String` (captured pairs) |
| Value | LANDED | `value` + `defaultValue` (both set); locale-aware numeric coercion (bytecode `coerceNumberForLocale`) |
| Enabled | LANDED | `enabled` (TRUE/FALSE strings) |
| AccessRights | LANDED | `Read_Write` accepted; `readOnly` stays false |
| OPCServer | LANDED | `opcServer` verbatim |
| OPCItemPath | LANDED | `opcItemPath` verbatim |
| ScanClass | DROPPED | `Default` not landed (`tagGroup` stays `""`) |
| DriverName | LANDED | **verbatim key `DriverName`** (non-canonical capitalized key rides the model) |
| ScaleMode | COERCED | `'1'` → `"Linear"` |
| RawLow | LANDED | `rawLow` (0-set indistinguishable from default; pair-proven via RawHigh) |
| RawHigh | LANDED | `rawHigh` 1000.0 |
| ScaledLow / ScaledHigh | LANDED | `scaledLow` 10.0 / `scaledHigh` 20.0 |
| ClampMode | COERCED | `'1'` → `"Clamp_Low"` |
| ScaleFactor | LANDED | `scaleFactor` 2.5 |
| Deadband | LANDED | `deadband` 0.25 |
| DeadbandMode | UNTESTED | — |
| FormatString | LANDED | `formatString` `"#,##0.0"` (**comma-bearing cell must be RFC-4180 quoted** — unquoted shifts every later column; captured both ways) |
| EngUnit | LANDED | `engUnit` `"PSI"` |
| EngLow | DROPPED | set `'0'`; absent from model |
| EngHigh | DROPPED | set `'100'`; export shows 0.0 (likely `EngLimitMode`-gated, untested) |
| EngLimitMode | UNTESTED | — |
| Tooltip | LANDED | `tooltip` |
| Documentation | LANDED | `documentation` |
| ExpressionType | COERCED | enum → valueSource: `1`→`expr`, `2`→`db`, `3`→`named_query`, `4`/`5`→`memory` (Expression still lands as a stray property on 4/5) |
| Expression | LANDED | `expression` (ET 1/3/4/5) or `query` (ET 2) |
| AlertMode … AlertDisplayPath (9 cols) | UNTESTED | every imported tag materializes `AlertAckMode: 0` / `AlertSendClear: 0` defaults regardless |
| OPCWriteBackServer / OPCWriteBackItemPath | UNTESTED | — |
| SQLBindingDatasource | UNTESTED | — |
| HistoryEnabled | LANDED | `historyEnabled` true |
| PrimaryHistoryProvider | LANDED | `historyProvider` |
| HistoricalScanclass | LANDED | `historyTagGroup` (`"Default Historical"`) |
| HistoricalDeadband | LANDED | `historicalDeadband` 0.5 |
| HistoricalDeadbandMode | COERCED | `'1'` → `"Percent"` |
| InterpolationMode | COERCED | `'1'` → `historicalDeadbandStyle: "Analog"` |
| HistoryMaxAgeMode | DROPPED | export keeps `historyMaxAgeUnits: "HOUR"` default |
| HistoryMaxAge | DROPPED | set `'60'`, export `historyMaxAge: 0` |
| HistoryTimestampSource | LANDED | **verbatim numeric** `HistoryTimestampSource: 1` (non-canonical key) |
| UDTParentType | LANDED | `typeId` (`"MotorType"`); UDT children (Amps/Doubled) RIDE from the type into the exported instance |
| PersistValue | LANDED | `persistValue` true |
| SourceDataType / SourceTagPath | UNTESTED | derived-tag columns |
| SQLBindingPollRate | DROPPED | in docs table, NOT in PROP_COLUMNS → silently ignored as unknown |
| `Permissions` (special header) | PARSED, **effectively DROPPED** | format `zone;role;RW\|RO` entries joined by `$` (bytecode-extracted; `Read_Write`/`Read_Only`/JSON all rejected with `Format of tag permissions model is illegal.`); `*;admin;RW` imports Good but lands `readOnly: false` + **EMPTY** `AllOf` read/write permission sets — no meaningful mapping survives |
| `AlarmStates` (special header, undocumented) | PARSED, **silently DROPPED** | format `name;SEVERITY;loLimit;hiLimit;flags;loTagPath;hiTagPath;timeDeadband;TIMEUNITS` entries joined by `$` (`MyAlarm;High;0;25;0;x;y;0;SEC` imports Good); severity enum has NO `Critical` (`No enum constant …AlertSeverity.Critical`); **the modern model carries NO alarms after import** — the live proof of the docs' "CSV format does not include support for alarm configurations", and stronger: the drop is SILENT |
| Modern properties (tagGroup, bindings, UDT parameter overrides, deadband modes, …) | DROPPED | not expressible: unknown columns silently ignored (proven with `TagGroup=MyTagGroup` / `NameX`) |

### Cross-cutting CSV findings

1. **Legacy-default materialization:** every CSV-imported atomic tag carries the full legacy-default property set (`AlertAckMode`, `AlertSendClear`, `deadband`, `rawHigh`, `scaledHigh`, `engHigh`, `formatString`, `historicalDeadband` + `historicalDeadbandStyle`, `historyMaxAge`, `historyTagGroup`, `tagGroup`) that configure-created tags do NOT have (cf. probe-1a's P11Seed T1 export). CSV import is not additive — it instantiates the whole legacy sheet. **[CORRECTED live 2026-09-14 by the 11-06 gate, Rig A 8.3.6 — see 11-LIVE-GATE.md §Deltas]** The sheet is PER-COLUMN, not universal: a column PRESENT in the CSV header materializes its model default; a column absent from the header materializes nothing. The "AlertAckMode/AlertSendClear … regardless" claim above overshot its evidence — the probe headers that showed Alert keys carried Alert*/AlarmStates columns; the 48-column docs-sample header (no Alert columns) materializes NO Alert keys on any CSV version marker (1 and 2 both re-tested live).
2. **Collision semantics:** `'o'` → `Good`; `'a'` on collision → `Bad_Failure("Tag 'ColX' already exists, and 'abort' collision policy has been specified")` — same QualityCode element pattern as XML (no exception), note the path is rendered WITHOUT the provider prefix in the CSV case.
3. **All-or-nothing:** any row-level error aborts the whole import; nothing partial ever lands (every failed target exported the `type:"Unknown"` skeleton).
4. **Rig B (8.3.3) parity: identical** — Path NPE verbatim, UDT-instance/AlarmStates/Permissions Good-shapes identical, `Incompatible CSV format (99)` verbatim, silent alarm drop, empty permission sets.
5. **Generator implication for 11-04:** the CLI's CSV download can only emit columns from this table's LANDED/COERCED set with numeric `TagType`/`DataType`/`ExpressionType` enums, `# version=1` marker, EMPTY `Path` cells (folders inexpressible), and must warn that alarms/permissions/scaling-adjacent columns (`EngLow`/`EngHigh`, `ScanClass`, `HistoryMaxAge*`) do not survive a CSV round-trip.

---

## Open-Question ledger (all five research questions → answered)

| Research Open Question | Answer | Section |
| --- | --- | --- |
| 2: kwargs `exportType='xml'` return form | Returns the full CRLF XML document string (no decl); positional form = filePath fallback | Probe 1 |
| 3: importTags basePath forms / QualityCode shape / temp write / collision | Subfolder AND provider root both work (no RpcContext constraint); `ArrayList[QualityCode]` str-shapes; JVM `/tmp` writable; collisions ride `Bad_Failure` elements, never exceptions | Probe 2 |
| 4: multi-level UDT XML byte shape + determinism | Full byte shape captured (artifact); (a) byte-deterministic, (b) round-trip = structural-only + UdtType import refusal | Probe 3 |
| 5: mixed-parent exportTags | Silent `type="Unknown"` corruption, pre-existing in the JSON baseline too | Probe 4 |
| 6 (CSV half): legacy CSV column coverage | Full table above; vocabulary = 51 PROP_COLUMNS + structural + 2 special headers; Path/Owner unusable; basePath ignored; alarms/permissions silently non-landing | Probe 5 |

---

*(Rig ops, teardown state = INTENTIONAL KEEP-ALIVE for 11-06: see 11-RIG-NOTES.md. Raw probe JSONs staged outside the repo at `/tmp/ign-p11-rigs/probe-outputs/`.)*
