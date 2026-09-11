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

*(§Probe 3 — multi-level UDT capture + determinism — and §CSV-Coverage follow in the Task-3 sections below.)*
