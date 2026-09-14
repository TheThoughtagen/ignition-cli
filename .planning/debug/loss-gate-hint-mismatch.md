# Debug Session: loss-gate refusal leaks file-read hint (UAT Phase 11, test 3)

- **Date:** 2026-09-14
- **Mode:** find_root_cause_only (research; no source changes)
- **Discovered:** UAT 2026-09-14 against live 8.3.6 rig (profile uat)
- **Repro:** Test 3 in `.planning/phases/11-tag-bulk-transfer-xml-csv/11-UAT.md`; fixture `.planning/phases/11-tag-bulk-transfer-xml-csv/artifacts/udt-multilevel.xml` with `--provider default --format xml`, no `--yes`.

## Symptoms (verbatim from UAT)

`ign tags import --file <lossy.xml> --provider default --format xml` (no `--yes`) refuses correctly — exit 2, `invalid_input`, accurate loss report on stderr — BUT the trailing hint line is wrong for this failure class.

Expected: the refusal's guidance should match the loss-gate failure ("re-run with --yes").

Actual:

```
error: invalid input: loss report (xml): the scan reports 2 finding(s) before the import:
  - [xml_export_edited_only] gateway-export-shaped file (MinVersion="8.0.0"): exports carry ONLY edited properties — unset properties ride defaults or the referenced UDT type
  - [xml_udt_type_definition] file contains UDT type definition(s) (type="UdtType") — importTags refuses them ("Udt definitions can only be imported in the UDT Definitions tab"), so the import lands nothing
top-level tag(s): P11UDT, MotorType
re-run with --yes to import anyway
hint: fix the input source — a readable file path via --file, or `-` to pipe the content on stdin
```

The file WAS perfectly readable. The `hint:` line is the generic file-read-failure hint leaking into the loss-gate path; the `re-run with --yes to import anyway` line above it is correct and is the appropriate guidance.

Errors: none — exit 2 with the misleading hint.

## Investigation steps

1. Located the throw site: `crates/ignition-cli/src/main.rs` — `loss_gate()` (line 2841). Confirmed the refusal is `CoreError::InvalidInput` with the prose report AS the `reason`, and the prose itself already ends with the correct guidance (`re-run with --yes to import anyway`) — main.rs:2862-2864.
2. Located the dispatch arm: `TagsCommand::Import` in main.rs:1385-1404 — `read_input_bytes` (line 1395) succeeds on the readable fixture, then `loss_gate` (line 1399) refuses and the `Err` is returned up the chain. So the file-read error path is NOT what fired — only its hint appears.
3. Located the render path: `render_error()` in `crates/ignition-cli/src/render.rs:161-174`. The human branch prints `error: {err}` then `hint: {err.hint()}` (render.rs:171-173). **Hints are attached centrally by error class, never at the throw site.**
4. Located `CoreError::hint()` in `crates/ignition-core/src/error.rs:626`. The `InvalidInput` arm (error.rs:649-661) has exactly two outcomes:
   - if `reason == TUI_TTY_REFUSAL_REASON` (exact string match on the sentinel const at error.rs:30) → the TTY-contextual hint ("run `ign tui` in an interactive terminal…", error.rs:654-655);
   - otherwise the default: **"fix the input source — a readable file path via --file, or `-` to pipe the content on stdin"** (error.rs:657-658).
5. Verified the loss-gate reason (`"loss report (xml): …\nre-run with --yes to import anyway"`) does not equal the TTY sentinel → default file-read hint fires. This is the misleading line.
6. Compared with other `InvalidInput` throws: ~66 sites across core (e.g. apicall auth-pattern refusal at `client/apicall.rs:79`, `parse_write_scalar` at main.rs:2771, time parsing at `actions/tags.rs:1155`). ALL share the same class-level file-read hint via this central arm; `tui_tty_refusal()` (error.rs:907-911) is the only sanctioned escape hatch, deliberately using the "same slug, same exit 2, only the hint differs" pattern (comment at error.rs:903-906).

## Root cause

The `hint:` is attached centrally per error class in `CoreError::hint()`, and the `InvalidInput` class carries a default hint that assumes a file-read/parse failure. The TAGS-12 loss gate reuses `InvalidInput` (correctly — exit 2 / `invalid_input` is a frozen taxonomy requirement) and embeds its own actionable guidance inside the message prose, but because its `reason` string doesn't match the one sentinel (`TUI_TTY_REFUSAL_REASON`), `hint()` falls through to the generic default at `crates/ignition-core/src/error.rs:657-658`.

Evidence chain:

| Step | Location |
|---|---|
| Loss-gate throw (`InvalidInput`, prose as reason) | `crates/ignition-cli/src/main.rs:2862-2864` |
| Central hint attachment (`hint: {err.hint()}`) | `crates/ignition-cli/src/render.rs:171-173` |
| `InvalidInput` hint arm: sentinel exact-match else default file-read hint | `crates/ignition-core/src/error.rs:649-661` (default at 657-658) |
| The one sanctioned contextual-override precedent (`tui_tty_refusal`) | `crates/ignition-core/src/error.rs:903-911`, sentinel const at error.rs:30 |

The generic file-read hint exists because `InvalidInput` was historically dominated by unreadable-file/malformed-input failures (`read_input_bytes` main.rs:2787, `read_json_input` main.rs:2817). The loss gate is the first refusal whose message already contains complete, correct guidance, which is why the class-default mismatch becomes visible in UAT.

## Suggested fix direction

Follow the established TTY-refusal precedent rather than re-architecting hint attachment. The idiomatic option, given core cannot see main.rs's prose, is to promote the loss-gate refusal into a `CoreError` constructor (mirroring `tui_tty_refusal()` at error.rs:907) — e.g. `CoreError::loss_gate_refusal(prose)` that builds `InvalidInput` with a stable sentinel reason (or reason suffix) such as `LOSS_GATE_REFUSAL_MARKER`, and branch on that sentinel inside the `InvalidInput` arm of `hint()` (error.rs:649) to return the `--yes` hint (or `None`, since the message already carries the guidance). Simpler variant: suffix/starts-with match on `"loss report ("` in the hint arm — same result, less churn, slightly less clean. A larger redesign (attaching hints at the throw site, or adding an optional `hint_override` field to `InvalidInput`) would also fix the fact that the file-read hint is arguably wrong for several of the ~66 other `InvalidInput` sites (auth-pattern refusal, `--value` array refusal, time-parse refusals), but that is a contract change beyond this UAT gap; plan-phase should scope to the sentinel/constructor fix and optionally note the broader hint-drift issue for a later phase.
