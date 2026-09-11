# Phase 11: Tag Bulk Transfer XML/CSV - Research

**Researched:** 2026-09-11
**Domain:** Ignition 8.3 tag export/import wire formats (XML/CSV), WebDev route extension, byte-faithful passthrough, loss-report warnings
**Confidence:** HIGH on architecture and gateway API facts (official 8.3 docs + Phase-5 live-proven route behavior); MEDIUM on exact XML-kwargs behavior and multi-level UDT XML byte shape (explicit live-probe items — the roadmap's own flag)

<user_constraints>
## User Constraints (from CONTEXT.md)

No CONTEXT.md exists for this phase — no locked user decisions. Per the phase spawn: all implementation choices (library selection, CLI surface, warning design) are open for research to recommend.

Standing constraints inherited from the milestone (REQUIREMENTS.md anti-scope, binding on this phase):

> | CLI-side XML/CSV re-serialization of tag exports | The kindling-shaped rabbit hole — passthrough only (TAGS-10/11); re-serialization corrupts fidelity |

Roadmap flag (binding): **"RESEARCH REQUIRED for this phase — pull a REAL multi-level UDT export and decide quick-xml serde-derive vs hand-rolled Event-loop before writing code (wiremock fixtures cannot provide this); validate the loss-report design against real exports. If routes change: one atomic WebDev bundle bump, both-direction version-drift tests."**
</user_constraints>

## Summary

Phase 11's architecture resolves cleanly once one asymmetry is faced: **the gateway can produce XML but cannot produce CSV.** Official 8.3 docs (docs.inductiveautomation.com, "Exporting and Importing Tags"): *"Tags can be imported from CSV, JSON, and XML. Tags can only be exported in XML or JSON"* and, verbatim: *"Although Ignition can import tags from a CSV format, Ignition does not export tags to a CSV format."* The scripting surface confirms it: `system.tag.exportTags([filePath], tagPaths, [recursive], [exportType])` takes `exportType` of `"json"` or `"xml"` only. So TAGS-10 (XML) is achievable as true gateway-byte passthrough in BOTH directions; TAGS-11 (CSV) is achievable as byte-faithful **upload** (bytes handed verbatim to the gateway's own parser via `system.tag.importTags`) plus **CLI-generated download** from the gateway's JSON interchange — the only possible CSV download — with the lossy-field behavior documented and warned. The requirement's own phrase "documented lossy-field behavior" only makes sense when a conversion happens, and no conversion is possible on a passthrough; the planner should lock this CSV interpretation (see Open Questions #1).

**XML direction (TAGS-10):** the existing `tagConfig` route gains an `exportType` parameter on its `exportTags` action (returning the XML as **base64** through the JSON envelope — byte-exact by construction, immune to JSON string-escaping edge cases) and a new `importTagsFile` action that receives base64 file bytes, writes them to a gateway-side temp file, calls `system.tag.importTags(filePath, basePath, collisionPolicy)` — the gateway parses its own format — deletes the temp file, and returns the QualityCode strings. The CLI never parses XML to transfer it; it parses ONLY to warn (TAGS-12). The roadmap's hypothesized `/data/status/tagexchange` and `/data/tagexchange` endpoints have **zero public footprint** (no results in official docs, no IA forum hits, nothing in the local `83-api` Bruno/Postman collection, nothing in ignition-mcp) — and the established, live-proven Phase-5 own-WebDev-routes architecture already provides a strictly better vehicle. Retire the tagexchange hypothesis; if desired, one curl probe on the rig can record its absence for the record.

**Parsing & the loss report (TAGS-12):** because transfer is passthrough, XML parsing exists ONLY for the advisory loss report — a lenient structural scan (count tags, collect `type` attributes, detect `CompoundProperty`/alarms, top-level names for the collision pre-check). This kills serde-derive as the tool: IA's XML uses dynamic `<Property name="valueSource">…</Property>` elements whose names are attribute VALUES (unmappable to fixed serde struct fields), and a strict schema parser would refuse files the gateway itself might accept — the wrong posture when the parse is advisory and the gateway is the parsing authority. **Recommendation: quick-xml 0.41 pull-Reader event loop** (~100 lines, `Event::Start`/`Event::Empty` matching on `Tag`/`CompoundProperty`/`Property`, no serde feature). CSV parsing for the loss report uses the `csv` crate (also needed for CSV generation).

**Bundle bump is mandatory:** any `doPost.py` change forces `ROUTE_VERSION 1.1.0 → 1.2.0` in all three pinned copies (each route's constant, `webdev/mod.rs::ROUTE_BUNDLE_VERSION`, `webdev/routes/VERSION`) — the existing `route_sources_carry_the_embedded_handshake_constants` test enforces the atomic bump. Both-direction drift is already machine-covered: new CLI + old routes → `route_version_mismatch` refusal (live-proven in Phase 9 for 1.0.0→1.1.0); old CLI + new routes → the same mismatch refusal via version comparison. Wiremock precondition mocks serve `ignition_core::webdev::ROUTE_BUNDLE_VERSION` dynamically, so existing contract tests auto-track the bump — zero test-fixture blast radius.

**Primary recommendation:** XML = gateway-byte passthrough both directions via two new/extended tagConfig route actions (base64 envelope, gateway-side temp files, `exportType='xml'` / `importTags`); CSV = CLI-generated download from the JSON interchange (documented lossy, stderr warning) + byte-faithful upload through `importTags`; quick-xml Event-loop scan (no serde) for the TAGS-12 loss report gated by `require_confirmation`-style `--yes` with exit-2 `invalid_input` refusal; one atomic bundle bump to 1.2.0; real multi-level UDT XML captured on the 8.3.3/8.3.6 rig recipe as the FIRST planning-gate task, with sha256 + re-export-byte-identity as the fidelity oracle.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| quick-xml | 0.41.x | Lenient XML event-scan for the TAGS-12 loss report + collision pre-check names | High-performance pull Reader; no schema needed; dynamic `<Property name=…>` elements are trivial in an event loop, painful in serde-derive; tiny dep (workspace lean-deps constraint) |
| csv | 1.x | CSV generation (download) + CSV loss-report scan (import) | Ubiquitous, tiny; handles embedded newlines/quoting (the legacy format's Expression/Documentation columns contain raw newlines — hand-rolled quoting is a trap) |
| (none for XML/CSV transfer itself) | — | Transfer is raw-byte passthrough through the existing reqwest pipeline | Byte fidelity is the REQUIREMENT; any codec in the transfer path is the bug |

### Supporting (already in graph — verified)
| Library | Purpose | Note |
|---------|---------|------|
| serde_json | envelope + JSON interchange (unchanged) | existing |
| base64 (route-side Jython + Rust) | byte-faithful carrier through the JSON envelope | Jython: `base64.b64encode`; Rust: the `base64` crate (check workspace — if absent, hex encoding is an equally valid zero-dep alternative for the route; Jython has `binascii.hexlify` built in) |
| wiremock / assert_cmd / tempfile | test conventions | existing |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| quick-xml Event-loop | quick-xml serde-derive (`serialize` feature) | REJECTED: dynamic Property names (`<Property name="valueSource">`) can't map to struct fields; strict schema hard-fails advisory scans; the parse must never block a file the gateway would accept |
| quick-xml | hand-rolled XML scan | Rejected per roadmap framing — quick-xml IS the event-loop option; hand-rolling quoting/entities/encodings buys nothing over a 200-line dep (contrast with the zip-surgery precedent: that was stdlib `zip`, this has no stdlib equivalent) |
| quick-xml | roxmltree / xmltree | Rejected: DOM-building parsers load whole trees for what is a flat scan; quick-xml is the Rust ecosystem default for streaming |
| Gateway XML export | CLI-side XML generation from JSON interchange | REJECTED — this is exactly REQUIREMENTS.md's anti-scope "kindling-shaped rabbit hole": XML↔JSON encodings diverge non-mechanically (JSON `{"bindType":"parameter","binding":"ns\u003d1;…"}` vs XML `<Property name="opcItemPath" boundValueType="parameter">ns=1;…</Property>`; alarms as array vs `CompoundProperty`/`PropertySet`); fidelity would be silently corrupt |
| Gateway CSV export | — (does not exist) | Official docs: gateway exports XML/JSON only. CSV download MUST be CLI-generated; document it as such |
| csv crate | manual CSV writer | Rejected: legacy values contain embedded newlines/commas/quotes (official sample shows multi-line Expression cells); RFC-4180 quoting edge cases are exactly what the crate owns |

**Installation:**
```bash
cargo add quick-xml@0.41 --package ignition-core   # default features; do NOT enable "serialize"
cargo add csv@1 --package ignition-core
```
(Verify exact latest patch versions at execution time; both are stable-line crates.)

## Architecture Patterns

### Recommended Change Surface (family extension, zero new crates — milestone ARCHITECTURE.md feature #5 confirmed)
```
crates/ignition-core/
├── webdev/routes/.../cli/tagConfig/doPost.py   # MODIFIED: exportTags gains exportType (xml → base64);
│                                               #           NEW importTagsFile action (base64 in → temp file →
│                                               #           system.tag.importTags → quality codes → temp deleted)
├── webdev/mod.rs                                # ROUTE_BUNDLE_VERSION 1.1.0 → 1.2.0 (three-way pin bumps with routes/VERSION)
├── src/actions/tags.rs                          # tags_export: format param; xml = raw-bytes passthrough (NO parse/normalize);
│                                               # tags_import: format param; xml/csv → importTagsFile route call;
│                                               # NEW loss_report scan module (pure fns: quick-xml event scan / csv row scan)
└── tests/tags_contract.rs                       # wiremock pins for the new route bodies (base64 round-trips)
crates/ignition-cli/
├── src/cli.rs                                   # --format json|xml|csv on tags export + tags import (flag on existing leaves —
│                                               # NO new tui_coverage registry rows)
├── src/main.rs                                  # dispatch: format resolution, loss-gate (--yes) pre-resolution
├── src/render.rs                                # loss-report stderr rendering; xml/csv stdout = RAW BYTES (no pretty)
└── tests/                                       # goldens (json default unchanged), loss-gate refusals, live gate extension
```

### Pattern 1: Byte-Faithful Passthrough (the phase's core invariant)
**What:** gateway-produced bytes travel response→file/stdout with zero decode/re-encode between. The route returns base64 of the exact bytes it produced; the CLI writes `base64::decode(bytes)` raw to the file/stdout.
**When to use:** XML download; CSV upload (direction reversed: file bytes → base64 → route → temp file → `importTags`).
**Why base64 and not a JSON string:** JSON strings are Unicode-safe and would *probably* round-trip valid UTF-8 XML unharmed — but "probably" is not the phase's contract, and JSON escaping normalization (e.g. lone-surrogate sanitation) is a silent-corruption class. Base64 is byte-exact by construction and trivially assertable (sha256 both ends).

```python
# Route side (Jython, tagConfig doPost.py — new exportTags behavior):
import base64
payload = system.tag.exportTags(tagPaths=paths, exportType=fmt)  # LIVE-PROBE 1: kwargs+xml returns string?
if isinstance(payload, unicode): payload_bytes = payload.encode('utf-8')
else: payload_bytes = str(payload)
return ok({'payload_b64': base64.b64encode(payload_bytes), 'format': fmt})
```
```rust
// Action side — the passthrough seam:
let raw = base64_decode(payload_b64)?;              // no XML knowledge here
std::fs::write(out, &raw)?;                          // file mode: raw bytes, no trailing newline added
// stdout mode: raw bytes on stdout — the sanctioned exception extended (byte-exact, never "pretty")
```

### Pattern 2: Gateway-Side Temp File Import (`importTagsFile` route action)
**What:** `system.tag.importTags(filePath, basePath, collisionPolicy)` is a FILE-PATH API (Phase-5 live-proven: payload form fails with "Import file not found"). The route receives base64 bytes, writes a gateway temp file, imports, deletes.
**When to use:** XML and CSV upload.
```python
# Route side (new action):
import base64, os
from java.io import File
tmp = File.createTempFile('ign-import', '.' + fmt)   # absolute path; JVM temp dir — no cwd ambiguity
try:
    fh = open(tmp.getAbsolutePath(), 'wb'); fh.write(base64.b64decode(data['file_b64'])); fh.close()
    results = system.tag.importTags(tmp.getAbsolutePath(), basePath, policy)
finally:
    tmp.delete()                                      # cleanup on EVERY path (finally, not except)
return ok({'results': [str(x) for x in results]})
```
**Gotcha:** collision policies documented for `importTags` are `'a'`/`'o'`/`'i'` (Ignore is new vs configure's a/o/m). The CLI matrix stays LOCKED at abort/overwrite — do not surface `'i'`.

### Pattern 3: Loss Report as a Pure Advisory Scan (TAGS-12)
**What:** pure functions in ignition-core: `scan_xml(raw: &[u8]) -> ExportScan` / `scan_csv(raw: &[u8]) -> CsvScan`, producing a structured report (tag count, types present, alarms present/absent, columns seen vs expected, top-level names). **The scan NEVER gates parsing validity** — an unparseable file is the gateway's error to make; the CLI reports what it could see and proceeds (the route's error envelope comes back honest).
**Lossy facts to report (from official 8.3 docs — HIGH confidence):**
- CSV: "the CSV format does not include support for alarm configurations" — any alarm-bearing source loses alarms at CSV generation; state it when generating CSV from a payload that contains `alarms` keys, and state it unconditionally at CSV import (alarms cannot arrive).
- CSV: legacy 7.x-era column set only (`Path,Name,Owner,TagType,DataType,Value,Enabled,AccessRights,OPCServer,OPCItemPath,ScanClass,…,UDTParentType,PersistValue,SourceDataType,SourceTagPath,SQLBindingPollRate,Permissions` — 47 columns + `# version=1` marker row). Modern tag properties outside this map (tagGroup? bindings beyond OPC-writeback? parameters/UDT definitions beyond SourceTagPath?) are dropped — the exact column-by-column coverage map is a **live-probe deliverable** (Probe 5).
- CSV: numeric legacy enums — TagType (0=OPC, 1=DB/Expression/Query per ExpressionType, 2=Client, 6=Folder, 10=UDT Instance, 13=Derived), DataType (0..9), AccessRights — a coercion class the report must name.
- XML: exports carry only properties "edited in at least one of the tags in the selected export folder" (official docs) — so an XML file round-tripped through export is not a complete tag model; the report should note this when scanning an export-shaped file.
- Gate UX: mirror `require_confirmation` exactly — print the report to STDERR (humans) + structured `loss_report` in the envelope data (agents), refuse exit-2 `invalid_input` with "re-run with --yes to import anyway" unless `--yes`. Pre-resolution, zero wire work. CSV **generation** (export direction) is non-destructive → warn-and-continue, no gate.

### Pattern 4: One Atomic Bundle Bump (routes change → mandatory)
1. Bump `ROUTE_VERSION = '1.2.0'` in **all four** doPost.py files, `ROUTE_BUNDLE_VERSION` in `webdev/mod.rs`, and `webdev/routes/VERSION` — ONE commit/PR; the existing three-way pin test (`route_sources_carry_the_embedded_handshake_constants`) fails on any partial bump = the atomicity mechanism.
2. `MIN_CLI` stays `'1.0'`.
3. Both-direction drift tests (machine-covered already; pin explicitly in the plan):
   - New CLI ↔ old deployed routes → `require_routes` version mismatch → exit 6 `route_version_mismatch` naming `ign webdev deploy` (Phase-9 live-proven for 1.0.0→1.1.0).
   - Old CLI ↔ new deployed routes → same mismatch refusal from the old binary's comparison.
4. Redeploy is REQUIRED before any Phase-11 command works on a rig (`ign webdev deploy`) — the live gate must deploy the new bundle itself (the `live_webdev_deploy_status_scriptexec_loop` pattern).
5. Wiremock blast radius: zero — `mount_precondition_ok` serves `ignition_core::webdev::ROUTE_BUNDLE_VERSION` dynamically (verified: tags_contract.rs:241).

### Anti-Patterns to Avoid
- **Any codec in the transfer path:** parsing-then-re-serializing XML to "normalize" it before writing = the anti-scope rabbit hole. The CLI writes bytes it received; the only parse is the advisory scan.
- **serde-derive on the IA tag XML:** dynamic Property names + advisory-parse posture make derive the wrong tool (see Standard Stack).
- **CLI-side CSV→JSON conversion on import:** CSV upload must reach `importTags` as bytes. Converting CSV to the JSON interchange CLI-side re-introduces the fidelity hole and duplicates the gateway's own legacy-column mapping.
- **Silent pretty-printing of XML/CSV on stdout:** `-o -`/stdout mode is byte-exact raw output; `serde_json::to_string_pretty` or added trailing newlines on XML files break the sha256 oracle. (JSON keeps its existing pretty behavior — untouched default format.)
- **Surfacing `importTags`'s `'i'` (Ignore) policy:** the collision matrix is LOCKED at abort/overwrite (03-02); note Ignore exists in the README, don't add it.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| XML event scanning | regex/manual lexer | quick-xml Reader | entities, encodings, self-closing tags, attribute quoting — all owned by the crate |
| CSV write/read | manual quoting | csv crate | embedded newlines in legacy values (official sample has multi-line Expression cells); RFC-4180 edge cases |
| Legacy CSV column mapping | invented schema | official docs column table + live-probe verification (Probe 5) | the 47-column legacy layout is a documented 7.x artifact with numeric enums; guessing types corrupts imports |
| Temp-file lifecycle (route) | fixed paths / hope | `java.io.File.createTempFile` + try/finally delete | gateway FS permissions + cwd ambiguity; leaked temp files on a production gateway are a real failure mode |
| Version-drift protection | new checks | existing three-way pin + `require_routes` | the machinery exists and is live-proven both directions |

**Key insight:** the phase's entire fidelity story is "the gateway does everything semantic; the CLI moves bytes and warns." Every hand-rolled parser/serializer weakens that story.

## Common Pitfalls

### Pitfall 1: The docs' "Returns: Nothing" contradicts the live-proven kwargs form
**What goes wrong:** planning against the documented signature (`filePath` positional, returns Nothing) instead of the live-proven 8.3 behavior — `system.tag.exportTags(tagPaths=paths)` (kwargs-only, no filePath) RETURNS the JSON payload string (route-verified Phase 5, `doPost.py:179`).
**How to avoid:** treat the return-string kwargs form as the primary hypothesis and **Probe 1** (does `exportType='xml'` ride the kwargs return form?) as the FIRST rig task; bake the temp-file fallback into the route regardless: if the kwargs+xml form errors or returns non-string, the route falls back to `exportTags(tmpfile, tagPaths, exportType='xml')` + read bytes + delete. Design both paths in one action.
**Warning signs:** wiremock tests pass while the live gate fails on the very first XML export.

### Pitfall 2: "Same parent folder" constraint on exportTags
**What goes wrong:** official docs: "All tag paths in the list must be from the same parent folder." The current CLI export accepts arbitrary multi-paths; XML export with mixed-parent paths may fail (or silently drop) where the JSON interchange tolerated it.
**How to avoid:** Probe 4 on the rig; if the constraint bites, refuse pre-resolution (exit 2, naming the offending parents) rather than shipping a gateway error. Note the existing JSON behavior must remain byte-identical — any constraint handling gates the NEW format only if it's genuinely new behavior.

### Pitfall 3: JSON-envelope string corruption of binary-ish payloads
**What goes wrong:** passing the XML through the envelope as a JSON string mostly works — until an encoding edge (control chars, BOM, non-UTF-8 declaration) silently normalizes.
**How to avoid:** base64 both new payloads (`payload_b64` out, `file_b64` in); sha256 assertions in the live gate make any corruption loud. Also: XML file mode writes NO trailing newline (the current JSON path appends `\n` — keep that JSON-only or the byte oracle breaks).

### Pitfall 4: The `# version=1` marker row and numeric enums in legacy CSV
**What goes wrong:** generating CSV without the `# version=1` marker row, or writing modern string enums ("AtomicTag", "Int4") into TagType/DataType columns, produces files the gateway's importer rejects or misreads (TagType 6=Folder, 0=OPC, 1=DB-per-ExpressionType, 10=UDT Instance, 13=Derived; DataType 0-9 per docs table).
**How to avoid:** generation writes the marker row + numeric enums per the official property table; the unit-test fixture set includes one of each enum class. Validate by round-tripping generated CSV through the rig (Probe 5 closes the mapping).

### Pitfall 5: Forgetting the redeploy sequencing in tests and docs
**What goes wrong:** after the 1.2.0 bump, every rig assertion of new verbs fails with `route_version_mismatch` until `ign webdev deploy` runs; CI goldens and README exit tables drift if the refusal shape changes.
**How to avoid:** live gate self-deploys first (established pattern); no new exit codes or slugs (loss-gate refusal = exit 2 `invalid_input`); README rows added in the same tasks as the flags (the Three-Place/two-places rule).

### Pitfall 6: Large exports vs the client timeout
**What goes wrong:** multi-thousand-tag XML exports can exceed the 30s reqwest default — the Phase-9 bundle-download lesson (`BUNDLE_DOWNLOAD_TIMEOUT=300s`, the ONE streaming site) applies in spirit.
**How to avoid:** check the `webdev_route_call` timeout posture; if a long-running export is plausible on real provider sizes, parameterize the timeout for the export call with a pinned constant + unit test at birth (09-05 pattern), or document the bound honestly.

### Pitfall 7: Stdout purity leaks from the loss report
**What goes wrong:** the loss report is diagnostic prose — a single report line on stdout in `--json` mode violates the byte-exact stdout purity harness (Phase-8: one stray byte fails CI).
**How to avoid:** report prose rides stderr only; structured `loss_report` rides envelope `data`; the existing harness covers the regression.

### Pitfall 8: snapbox golden escaping on new goldens
**What goes wrong:** `str!` backslash normalization mangles goldens containing `\` (documented 03-02/05-05 gotcha) — XML fixtures and loss-report messages will contain quotes/backslashes.
**How to avoid:** keep golden text simple; record escapes inline at the golden (established convention).

## Code Examples

### The XML sample the gateway actually produces (official docs — the shape the scan must handle)
```xml
<Tags MinVersion="8.0.0" locale="en_US">
   <Tag name="Amps" type="AtomicTag">
      <Property name="opcItemPath" boundValueType="parameter">ns=1;s=[Dairy]_Meta:Overview/Motor {MotorNumber}/Amps</Property>
      <Property name="valueSource">opc</Property>
      <Property name="historyProvider" datatype="String">MySQL</Property>
      <CompoundProperty name="alarms">
         <PropertySet>
            <Property name="mode">3</Property>
            <Property name="setpointA">25</Property>
            <Property name="name">Low Amps</Property>
            <Property name="priority">4</Property>
            <Property name="displayPath" bindtype="Expression">Motor{MotorNumber}</Property>
         </PropertySet>
      </CompoundProperty>
      <Property name="historyEnabled" datatype="Boolean">true</Property>
   </Tag>
</Tags>
```
Scan observations: root attrs (`MinVersion`, `locale`) worth reporting; `type` attr values mirror tagType; alarms live under `CompoundProperty` — presence/absence detection is one event match. NOTE: a multi-level UDT XML sample (UdtType/UdtInstance nesting, parameters encoding) does NOT appear in official docs — it is **Probe 3's** capture deliverable (the roadmap's explicit flag).

### The loss-scan event loop (recommended shape)
```rust
// Source: quick-xml 0.41 README pattern + IA 8.3 Tag File Formats docs
use quick_xml::events::Event;
use quick_xml::reader::Reader;

pub fn scan_xml(raw: &str) -> XmlScan {
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut scan = XmlScan::default();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"Tag" => { scan.tags += 1; if let Some(t) = e.try_get_attribute("type")? { scan.types.push(t.unescape_value()?.into_owned()); } }
                b"CompoundProperty" => scan.has_compound = true,   // alarms in practice
                b"Property" => { /* name attr tally for the report */ }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => { scan.partial = true; break; }   // ADVISORY: report what we saw, never refuse
            _ => {}
        }
        buf.clear();
    }
    scan
}
```

### Legacy CSV skeleton (generation target — official docs sample verbatim columns)
```csv
Path,Name,Owner,TagType,DataType,Value,Enabled,AccessRights,OPCServer,OPCItemPath,ScanClass,DriverName,ScaleMode,RawLow,RawHigh,ScaledLow,ScaledHigh,ClampMode,ScaleFactor,Deadband,DeadbandMode,FormatString,EngUnit,EngLow,EngHigh,EngLimitMode,Tooltip,Documentation,ExpressionType,Expression,OPCWriteBackServer,OPCWriteBackItemPath,SQLBindingDatasource,HistoryEnabled,PrimaryHistoryProvider,HistoricalScanclass,HistoricalDeadband,HistoricalDeadbandMode,InterpolationMode,HistoryMaxAgeMode,HistoryMaxAge,HistoryTimestampSource,UDTParentType,PersistValue,SourceDataType,SourceTagPath,SQLBindingPollRate,Permissions
# version=1,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,
,Memory Tag,,1,7,I'm a memory Tag,TRUE,Read_Write,...,FALSE,,,,,,,,,,,,
A Folder/,OPC in a folder,,0,2,,TRUE,Read_Write,Ignition OPC-UA Server,[devicename]folder/path,...,,,FALSE,,,,,,,,
```
Folder membership rides the `Path` column (`A Folder/` prefix), `_types_` rows carry UDT definitions, UDT instances use `UDTParentType`/`SourceTagPath`.

### Live-gate fidelity oracle (the strongest byte-faithful proof — CLI-independent)
```text
1. export XML of a multi-level UDT subtree → sha256 A   (gateway bytes)
2. import that exact file into a scratch provider (--yes after loss report)
3. re-export the imported subtree → sha256 B
4. assert A == B (byte-identical re-export proves: CLI moved bytes unmutated AND
   the gateway's own parser reconstituted the model its own exporter describes)
5. assert file-on-disk sha256 == route-response base64 sha256 (transport fidelity)
```
Caveat to verify at Probe 3: exports must be deterministic for unchanged tags (the "edited properties only" rule implies stability; if any timestamp sneaks in, fall back to structural comparison + sha256-vs-transport only).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Milestone ARCHITECTURE.md Pattern 4: CLI-side edge conversion (normalize→render XML/CSV from JSON) | Gateway-byte passthrough for XML; CLI-generated CSV only because the gateway cannot | Phase 11 roadmap hardened the goal ("gateway bytes verbatim, never CLI-side re-serialization") | The edge-converter pattern is superseded for transfer; survives only as the CSV-download generator (JSON→legacy CSV), which is a conversion, documented and warned — not a passthrough claim |
| 7.x-era CSV as a first-class interchange | CSV demoted to a legacy IMPORT-only format in 8.x | Ignition 8.0 tag overhaul | CSV support is compatibility surface; the lossy-field warnings (TAGS-12) are the honest posture, not a nice-to-have |
| `importTags` payload-form (ignition-mcp prior art) | `importTags` is file-path-only; payload import rides `configure` | Phase-5 live proof | The new route action MUST use the temp-file form |
| Route bundle 1.1.0 | 1.2.0 (this phase) | Phase 11 | Redeploy required; both-direction drift refusal already live-proven |

**Deprecated/outdated for this phase:**
- `/data/status/tagexchange` + `/data/tagexchange` hypothesis: zero footprint in official docs, IA forum, local 83-api collection, and ignition-mcp. Retire from the plan; optional one-curl absence probe on the rig for the record (LOW effort, LOW value).
- serde-derive XML: rejected (dynamic Property names; advisory-parse posture).

## Open Questions

1. **CSV download vs the "server-byte-faithful" requirement wording (PLANNER MUST LOCK)**
   - What we know: the gateway cannot export CSV (official docs, HIGH). TAGS-11 + success criterion 2 say CSV download with "server-byte-faithful passthrough" — mutually impossible as literally written; the requirement's own "documented lossy-field behavior" phrase implies transformation happens.
   - Recommendation: lock the composite reading — XML = full passthrough (download AND upload); CSV = upload is byte-faithful to the gateway's parser, download is CLI-generated from the gateway's JSON interchange with documented lossy behavior; README states plainly "the gateway cannot export CSV; `--format csv` output is CLI-generated (lossy: no alarms, legacy columns)". This satisfies every success criterion honestly.
2. **Does the kwargs return-string form of `exportTags` accept `exportType='xml'`? (Probe 1)**
   - What we know: `tagPaths=` kwargs returns a JSON string (live-proven); docs list `exportType` as a parameter.
   - Recommendation: first rig task; route carries the temp-file fallback either way, so planning need not block on it.
3. **`importTags` behavior from a WebDev thread: accepted basePath forms (provider root vs subfolder), QualityCode return shape, temp-file write permissions (Probe 2)**
   - What we know: docs example imports into a subfolder (`[default]Imported Tags`); provider-root calls from WebDev threads fail for getConfiguration/exportTags (no RpcContext — the 07-06 lesson). importTags may or may not share that constraint.
   - Recommendation: probe both `[provider]` and `[provider]folder` basePaths; if provider-root is RpcContext-blocked, the CLI targets a subfolder or documents the constraint symmetric to 07-06's refusal.
4. **Multi-level UDT XML byte shape + export determinism (Probe 3 — the roadmap's explicit flag)**
   - What we know: official docs show AtomicTag XML only; UDT nesting/parameters encoding in XML is uncaptured.
   - Recommendation: capture from a real 8.3.3 + 8.3.6 rig (recipe below), check re-export determinism for the fidelity oracle.
5. **Mixed-parent path behavior on exportTags (Probe 4)** — see Pitfall 2.
6. **Legacy CSV column coverage of modern properties (Probe 5)** — generate→import→re-export JSON on the rig; the diff IS the documented lossy-field table.

## Sources

### Primary (HIGH confidence)
- Official Ignition 8.3 docs via Context7 (`/websites/inductiveautomation_8_3`):
  - `system.tag.exportTags` — `[filePath]` optional, `tagPaths` required, `exportType` "json"|"xml", same-parent constraint, kwargs support (docs/8.3/appendix/scripting-functions/system-tag/system-tag-exportTags)
  - `system.tag.importTags(filePath, basePath, [collisionPolicy])` — JSON/XML/CSV, policies a/o/i, returns QualityCode list (…/system-tag-importTags)
  - "Exporting and Importing Tags" page — CSV import-only statement, XML format sample, legacy CSV sample + property/enum tables, "edited properties only" export rule (docs/8.3/platform/tags/exporting-and-importing-tags)
  - JSON format examples incl. UDT instance shape (docs/8.3/platform/scripting/json-format)
- quick-xml via Context7 (`/tafia/quick-xml`): Reader event loop, 0.41 current, serde behind feature (not used)
- Codebase (read directly, HIGH): `webdev/routes/.../tagConfig/doPost.py` (route contract + live-proven call forms), `actions/tags.rs` (tags_export/tags_import + normalization + collision pre-check), `webdev/mod.rs` (three-way version pin + route-source tests), `cli.rs`/`main.rs` (TagsCommand Export/Import arms, read_json_input, require_confirmation exit-2 pattern), `tests/tags_contract.rs` (wiremock patterns; dynamic ROUTE_BUNDLE_VERSION in mocks), `e2e_webdev.rs` (live-gate env contract + round-trip gate pattern)
- `.planning/phases/05-*/05-RESEARCH.md` + `05-04/05-05-SUMMARY.md` — WebDev wire protocol, envelope contract, live-proven exportTags kwargs behavior, stdout-exception #4
- `.planning/phases/09-*/09-RIG-NOTES.md` + `09-LIVE-CAPTURES.md` — rig recipe, headless commissioning, token provisioning, both-direction bundle-drift live proof
- `.planning/research/ARCHITECTURE.md` — feature classification (flag-on-existing-leaf, zero new crates), anti-pattern 5 (bundle-bump economy)
- `.planning/REQUIREMENTS.md` — TAGS-10/11/12 verbatim + anti-scope line 72

### Secondary (MEDIUM confidence)
- DuckDuckGo + IA forum searches for `tagexchange`: zero relevant results (negative evidence only — absence of footprint, not proof of endpoint absence)

### Tertiary (LOW confidence — live-probe required)
- Exact XML bytes of a multi-level UDT export (header/encoding decl/indentation/UDT property encoding)
- `exportTags(tagPaths=…, exportType='xml')` kwargs return-form behavior
- `importTags` provider-root basePath acceptance from WebDev threads
- Legacy CSV full coverage map of modern 8.3 properties

## Metadata

**Confidence breakdown:**
- Standard stack (quick-xml event loop + csv crate): HIGH — crate facts via Context7; choice forced by format shape + advisory-parse posture
- Architecture (passthrough seams, route actions, temp-file import): HIGH — rides the live-proven Phase-5 route architecture; every mechanism (envelope, precondition, deploy, version pin) exists and is proven
- Gateway API facts (exportTags/importTags signatures, CSV import-only): HIGH — official 8.3 docs, cross-consistent with live route behavior
- XML-kwargs + UDT-XML byte shape + importTags-from-WebDev specifics: MEDIUM/LOW — the five probes, deliberately sequenced as the plan's first task per the roadmap flag
- Loss-report design: HIGH on mechanism (docs' lossy facts + existing guard patterns); MEDIUM on exact field table (Probe 5 closes it)

**Rig access for probes/live gates (verified available):** the Phase-9/10 recipe — `docker run inductiveautomation/ignition:8.3.6` @18188 + compose `8.3.3` @19188, headless commissioning wire recipe, `provision_token.sh`, `IGNITION_LIVE_URL/TOKEN/MUTATIONS` gate contract. Images cached locally; ~30s warm boot; both rig generations proven three times (09, 10).

**Research date:** 2026-09-11
**Valid until:** ~2026-10-11 (stable domain; re-verify quick-xml/csv patch versions at execution)
