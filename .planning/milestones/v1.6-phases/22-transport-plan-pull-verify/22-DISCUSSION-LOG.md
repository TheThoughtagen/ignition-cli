# Phase 22: Transport, Plan, Pull & Verify - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-14
**Phase:** 22-transport-plan-pull-verify
**Areas discussed:** Host key & transport select, Delete-set source, Pull behavior, Live-capture logistics

---

## Host key & transport select

| Question | Options | Selected |
|---|---|---|
| Host key source | Repo known_hosts / Inline fingerprint / User's ~/.ssh/known_hosts | Repo known_hosts |
| LocalFs selection | `local` block exclusive with `ssh` / CLI `--data-dir` / You decide | `local` block |
| key_env handling | Temp file 0600 / Drop key_env / ssh-agent load | Temp file 0600 |
| Timeouts | Fixed defaults now / Target field now / You decide | Target field now |

## Delete-set source

| Question | Options | Selected |
|---|---|---|
| Previously declared source | On-box ledger / `--base` render.json / Ledger + `--base` check | On-box ledger |
| Live-only resources | 5th status `unmanaged` / Separate list / Omit | `unmanaged` |
| Bad ledger | Refuse / Warn, empty delete set | Refuse |
| plan_sha256 binding | Render + live + delete set / You decide | Render + live + delete set |

## Pull behavior

| Question | Options | Selected |
|---|---|---|
| Embedded secrets | Skip + report / Refuse whole pull / Write with placeholder | Skip + report |
| Types | Manifest or generated / Manifest required / All non-denied | Manifest or generated |
| Existing tree | Refuse non-empty / `--out` elsewhere / Overwrite with `--yes` | Overwrite with `--yes` |
| Collection | Mode + core split / Effective view flat / You decide | Mode + core split |

**Notes:** Overwrite with `--yes` makes pull a guarded op that stays in MCP through the synthetic confirm.

## Live-capture logistics

| Question | Options | Selected |
|---|---|---|
| Who runs | Agent over SSH / Script, user runs / Mixed | Agent over SSH |
| Gating | C7a first, C6/OQ5 at exit / All first / Code first | C7a first, C6/OQ5 at exit |
| OQ5 tree | Pull from real gateway / 183-file corpus / Synthetic | Pull from real gateway |
| Host ready | Yes / Partly / Not yet | Not yet |
| C7a host | wh-win / Wait for WFLWVM01 / Provision in-phase | Wait for WFLWVM01 |
| SC-1 host | wh-win OK / WFLWVM01 only | WFLWVM01 only |

## Claude's Discretion

- `plan --diff` human format, verify log-scrape window, tag-probe timeout semantics, `ObservedState` shape, transport vs API per check, canonical plan serialization.

## Deferred Ideas

- wh-win as a C7a stand-in (declined).
- Ledger vs `--base` cross-check warning.
