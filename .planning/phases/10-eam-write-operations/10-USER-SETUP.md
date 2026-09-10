# Phase 10: User Setup Required

**Generated:** 2026-09-10
**Phase:** 10-eam-write-operations
**Status:** Incomplete

Complete these items to close SC-5 (the phase's fifth success criterion: one guarded
write live-verified end-to-end against the REAL WHK controller rig, recorded during the
phase). Claude automated everything possible — the gate exists in
`crates/ignition-core/tests/live_gateway.rs` (`live_eam_write_lifecycle`, commit
`1293113`) and its record lives in
[10-LIVE-GATE.md](./10-LIVE-GATE.md) (`status: blocked-on-env`); these items require
human access to the WHK controller gateway that Claude cannot provision.

## Environment Variables

| Status | Variable | Source | Add to |
|--------|----------|--------|--------|
| [ ] | `IGNITION_LIVE_URL` | The WHK controller gateway's base URL (e.g. `http://<whk-controller-host>:<port>`) — the same env the `#[ignore]` live suite has used since Phase 4 | Shell environment when running the gate (export in the session, or your shell profile) |
| [ ] | `IGNITION_LIVE_TOKEN` | An API token on the WHK controller **with EAM rights** — FULL `name:key` string (Platform → Security → API Keys; the header carries the complete `name:key`) | Shell environment when running the gate |

**Notes:**
- Values are never recorded in the repo — the gate echoes var NAMES only into the log.
- If the WHK controller's EAM module is not already in `installMode: "Controller"`,
  every `/data/eam/api/v1/*` operation answers 403 and the gate fails with a dedicated
  rig-config diagnostic (the headless flip recipe is in [10-RIG-NOTES.md](./10-RIG-NOTES.md),
  §"EAM controller-mode provisioning" — but per the captures' Decision 11 the CLI does
  not expose the flip; do it per your ops process).

## Dashboard Configuration

None — the gate creates, mutates, and deletes ONLY its own
`ign-live-scratch-{epoch}` task and proves deletion afterward.

## Verification

After completing setup, run the gate from the repo root:

```bash
cargo test -p ignition-core --test live_gateway live_eam_write_lifecycle -- --ignored --nocapture
```

Expected results:
- `test live_eam_write_lifecycle ... ok` with per-step log lines: `gate step create →
  flip → suspend → verify → resume → delete` and the cleanup-proof line
  (`post-delete find = not_found`).
- The default `cargo test -p ignition-core` stays green (the gate is `#[ignore]` — it
  never leaks into normal runs).
- Then record the run: append the per-step verbatim outcomes to §5 of
  [10-LIVE-GATE.md](./10-LIVE-GATE.md) and flip its status to `passed` (or
  `failed-with-findings`).

---

**Once all items complete:** Mark status as "Complete" at top of file.
