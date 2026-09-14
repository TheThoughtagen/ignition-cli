---
status: diagnosed
phase: 11-tag-bulk-transfer-xml-csv
source: 11-01-SUMMARY.md, 11-02-SUMMARY.md, 11-03-SUMMARY.md, 11-04-SUMMARY.md, 11-05-SUMMARY.md, 11-06-SUMMARY.md
started: 2026-09-14T10:43:27Z
updated: 2026-09-14T05:50:00Z
---

## Current Test
<!-- OVERWRITE each test - shows where we are -->

[testing complete]

## Tests

### 1. XML Bulk Export to File
expected: `ign tags export --format xml` writes a .xml file containing the gateway's verbatim XML document — 3-space indent, NO <?xml declaration, CRLF line endings, trailing CRLF. No JSON wrapper or summary text in the file.
result: pass

### 2. XML Export to stdout / Pipes
expected: `ign tags export --format xml -o -` writes the RAW XML bytes to stdout (byte-identical to the file from test 1). Piping (e.g. to a file or `wc -c`) sees pure XML bytes only — the human summary line goes to stderr, never stdout.
result: pass

### 3. Loss Gate Refuses Risky XML Import (TAGS-12)
expected: `ign tags import --format xml <file-with-udt-types-or-alarms.xml>` WITHOUT --yes refuses with exit 2 / invalid_input and prints a loss report on stderr naming what the format would drop (e.g. UDT type definitions, alarm configs). The refusal happens BEFORE any gateway request — it works even with no gateway reachable, and nothing is written.
result: issue
reported: "Gate fired correctly (exit 2, invalid_input; report named xml_udt_type_definition + xml_export_edited_only, top-level tags P11UDT/MotorType, 're-run with --yes') BUT the trailing hint is misleading: 'fix the input source — a readable file path via --file, or `-` to pipe the content on stdin' — the file WAS readable; the refusal is the loss gate, not a file-read failure. Generic invalid_input hint boilerplate leaks into the gate path."
severity: minor

### 4. XML Import with --yes
expected: The same import WITH `--yes` proceeds: tags land in the gateway (visible in Designer/browser), and in JSON mode the success envelope carries `data.loss_report` with the structured scan summary (facts + top-level names).
result: issue
reported: "Request fired; envelope accurate (imported:2, failed:[], data.loss_report present with facts + top_level_names) and live rig verified the tags DID land fully (MotorType type def with members under _types_, P11UDT instance with M1/M2/Sub children). BUT the xml_udt_type_definition fact text is factually WRONG on this rig: it claims importTags refuses UDT type definitions and 'the import lands nothing' — live truth is the opposite (full landing). The 11-01 capture claim is falsified for this path (fresh 8.3.6 provider, importTagsFile route); advisory text misleads users/agents on every UdtType-bearing import."
severity: minor

### 5. XML Round-Trip Fidelity
expected: Export a UDT subtree → import it into a fresh target provider (the UDT type definition must exist there) → re-export from the target: the two XML documents match after sibling-order normalization (order may permute, structure does not). Separately: re-exporting an UNCHANGED subtree is byte-identical to the previous export.
result: pass

### 6. CSV Export (CLI-Generated, Warn-and-Continue)
expected: `ign tags export --format csv` SUCCEEDS and writes a legacy-format CSV (48-column header + `# version=1` marker row). In human mode, lossy-field warnings appear on STDERR (no alarms, numeric enum coercion, dropped keys) — but the command does not fail.
result: pass

### 7. CSV Import
expected: `ign tags import --format csv <file>.csv` triggers the same loss gate (any non-empty CSV carries unconditional no-alarms/legacy-columns facts) — refuses without --yes, proceeds with --yes. After import: tags land at the PROVIDER ROOT (CSV basePath is ignored — documented), numeric enums coerced, no alarms/permissions land.
result: pass

### 8. Collision Guard on New Formats
expected: Importing XML/CSV that collides with an existing tag name under abort policy (default) refuses with exit 6 / tag_collision BEFORE anything is written; with the overwrite flag the import proceeds and updates the tags.
result: pass

### 9. JSON Default Unchanged
expected: `ign tags export` WITHOUT --format behaves exactly as before Phase 11 — byte-identical JSON output, default filename still .json. Old workflows show no visible difference.
result: pass

## Summary

total: 9
passed: 7
issues: 2
pending: 0
skipped: 0

## Gaps

- truth: "Loss-gate refusal reads as a loss-gate refusal — the accompanying hint matches the actual failure (loss findings), not generic invalid_input file-read boilerplate"
  status: failed
  reason: "User reported: gate fired correctly (exit 2, report named the UDT-type finding verbatim) but the trailing hint says 'fix the input source — a readable file path via --file, or `-` to pipe the content on stdin' although the file was readable; hint is invalid_input-class boilerplate leaking into the gate path"
  severity: minor
  test: 3
  root_cause: "Hints attach centrally per error class: render_error prints err.hint() (render.rs:171-173); CoreError::hint()'s InvalidInput arm (error.rs:649-661) defaults to the file-read hint for every reason that is not the exact-match TUI_TTY_REFUSAL sentinel — the loss gate's throw (main.rs:2862-2864) rides InvalidInput, so it inherits the file-read hint although its own prose already carries the correct 're-run with --yes' guidance. The TTY-refusal constructor (error.rs:907-911) is the one sanctioned same-slug/same-exit hint override — the precedent to follow."
  artifacts:
    - path: "crates/ignition-core/src/error.rs"
      issue: "InvalidInput hint arm (649-661) — generic file-read default; only escape is the TTY sentinel"
    - path: "crates/ignition-cli/src/main.rs"
      issue: "loss_gate throw (2862-2864) rides InvalidInput with no hint-override mechanism"
    - path: "crates/ignition-cli/src/render.rs"
      issue: "central hint attachment (171-173) — class-level, not throw-site"
  missing:
    - "Add a CoreError::loss_gate_refusal(prose) constructor mirroring tui_tty_refusal(): stamp the InvalidInput reason with a LOSS_GATE_REFUSAL_MARKER sentinel"
    - "Branch on the sentinel in hint()'s InvalidInput arm to return the --yes hint (or None — the message already carries guidance)"
    - "Note (out of scope): ~66 other InvalidInput sites share the generic hint; throw-site hint redesign deferred"
  debug_session: ".planning/debug/loss-gate-hint-mismatch.md"

- truth: "The xml_udt_type_definition loss fact accurately describes live importTags behavior for UDT type definitions"
  status: failed
  reason: "User reported: --yes import of the UDT fixture landed FULLY (envelope imported:2/failed:[] verified accurate against live rig — MotorType def with members under _types_, P11UDT instance with children), but the fact text claims 'importTags refuses them (\"Udt definitions can only be imported in the UDT Definitions tab\"), so the import lands nothing' — falsified live on 8.3.6 fresh provider via the importTagsFile route; contradicts the 11-01 capture claim"
  severity: minor
  test: 4
  root_cause: "The 11-01 probe-3(b) capture proved the verbatim refusal ONLY for FOLDER-basePath imports ([default]P11Roundtrip); the shipped CLI import path always sends the PROVIDER-ROOT basePath (format!([{provider}]), tags.rs:2210) where the gateway ACCEPTS UdtType definitions and routes them to [provider]_types_/Name. The capture was true but scope-wrong; the fact text (tag_loss.rs:172-180) generalized a folder-basePath behavior onto the only path the CLI exposes. Live probes (scratch provider uatcsv only) confirmed landing in all three conditions: type-only file, mixed fixture, and type-pre-existing re-import — ruling out any pre-existence condition. 11-06 silently worked around the stale claim (config-creates the type, instance-only seeded XML) so it survived to UAT."
  artifacts:
    - path: "crates/ignition-core/src/actions/tag_loss.rs"
      issue: "falsified detail text (172-180); doc comment (27-30); test comment (439-441 — has_fact assertion itself stays valid)"
    - path: "README.md"
      issue: "§The loss gate (~:1118): 'UDT type definitions the gateway refuses outright' repeats the unscoped claim"
    - path: ".planning/STATE.md"
      issue: "lines ~134 and ~143 repeat the unscoped claim as phase decisions"
    - path: ".planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md"
      issue: "§Probe 3 (b)(1)/oracle item 4 lack a scope marker (capture text accurate for its conditions — needs the Probe-5-style dated scope note)"
  missing:
    - "Keep the fact FIRING (detection is correct; real transfer caveats exist) but reword the detail to live truth: definitions DO import and route to [provider]_types_/Name; the verbatim refusal is folder-basePath-only (11-01 capture) and unreachable via this command; include 11-06 delta-4 caveats (provider-qualified udtParentType points at the source provider cross-provider; 8.3.3 silently drops parameter overrides when the target type is unresolvable)"
    - "Mirror the correction in README.md (loss-gate section) and STATE.md (dated corrections), add the captures-doc scope note, fix the tag_loss.rs test comment"
  debug_session: ".planning/debug/udt-type-fact-falsified.md"
