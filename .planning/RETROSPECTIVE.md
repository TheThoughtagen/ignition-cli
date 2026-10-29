# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.6 — Declared Gateway Deploy

**Shipped:** 2026-10-29
**Phases:** 6 | **Plans:** 92 | **Sessions:** not tracked

### What Was Built
- `DeployApi`: a library client for 8.3.6 config resources, scan-lock, scan and deployment modes, with every endpoint shape taken from a live capture
- `ign deploy render`: an offline, byte-faithful renderer whose 183-file corpus round-trips identically on Linux, macOS and Windows
- `ign deploy plan`, `pull` and `verify` over hardened system OpenSSH/sftp to Linux and Windows hosts, plus the local filesystem
- Guarded `ign deploy apply` and `rollback` with snapshot, `.gwbk`, measured scan-lock hold, auto-rollback, on-box journal and run store
- Redundant-pair verification of the backup without writing to it, and `verify --node both`
- Guarded `ign deploy projects` and a reusable `deploy-projects.yml` workflow usable from any organization

### What Worked
- Capture-first planning again caught wrong assumptions before code depended on them. Phase 20 overturned the research on the held-lock status (409 HTML, not 408), the filter syntax and a release route that does not exist.
- Measuring before committing to a design. The per-file Windows commit took 234-348 s for 152 files. That measurement drove the batched on-box script (6.92 s), which brought the hold to 176 s, inside the 300 s ceiling.
- Building both transport halves in Phase 22 but leaving the write half uncalled until Phase 23. The write path was written and tested once while only reads were at risk.
- Reusing one `restore_snapshot` for auto-rollback, manual rollback and journal recovery. Every live gate exercised the same code.
- Landing surface parity with each verb (TUI row, README and exit-table rows, GUARDED_OPS, MCP exclusion). Drift tests caught gaps at the phase where they appeared.

### What Was Inefficient
- Phase 25 grew from its first plan set to 27 plans. UAT on a real self-hosted Windows runner raised G-25-4 to G-25-13 (VC++ runtime, `autocrlf`, NETWORK SERVICE ACLs, the PS 5.1 `Move-Item` replace window, cross-org secret mapping), none of which the hermetic suites could see.
- CI fail-fast hid a `cfg(windows)` failure in Phase 22 (G-22-1). Six CI rounds followed before all four jobs were green.
- A shared global `CARGO_TARGET_DIR` let a worktree overwrite the main checkout's `ignition-core` artifacts. Tests vanished and imports failed in ways that looked like code defects. It is now a project rule: no worktrees, or a unique target dir per worktree.
- SUMMARY bookkeeping drifted. Phase 23 recorded most requirements under `requirements-advanced` only, so the milestone audit needed 11 manual checks. Some VERIFICATION body text went stale after later closures.
- Debug sessions and one quick task were never marked closed, which blocked the pre-close audit until quick 261029-fyc.

### Patterns Established
- One hash-checked on-box script per host (POSIX `sh` and PowerShell 5.1) behind a SHA-256-checking stub, with a shared conformance table run against every host
- A verdict envelope for post-write red outcomes (`ok:true`, `data.ok=false`, exit 6), kept separate from pre-write `CoreError` refusals
- A plan-hash handshake (`--plan-sha256`) that CI and agents must pass before any write
- Live gates on real hosts (Azure Windows Server 2022, a Linux ssh host, a two-node 8.3.6 pair) as the exit criterion for any transport-facing phase

### Key Lessons
1. Put a real target-platform runner (the Windows self-hosted runner) in UAT before the last phase's plans are written. Most Phase 25 gap plans came from runner environment facts.
2. Turn off fail-fast in the CI matrix while a platform-specific leg is new. Otherwise one leg's failure hides another's.
3. Mark `requirements-completed` in the plan that finishes a requirement, not only `requirements-advanced`. The audit can then use the SUMMARY files as its source instead of manual checks.
4. Close debug sessions and quick tasks in the plan that resolves them. A pre-close audit should find nothing new.

### Cost Observations
- Model mix: not tracked
- Sessions: not tracked
- Notable: 524 commits over 20 days. Live-rig and Windows-host windows, not code, set the pace in Phases 23 and 25.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.0 | — | 7 | Agentic JSON contract and exit taxonomy frozen from Phase 1 |
| v1.1 | — | 7 | Capture-first planning; evidence-ledger checkpoints |
| v1.6 | — | 6 | Measured hold budgets; live gates on Windows and Linux hosts; pre-close artifact audit |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| v1.0 | 863 | — | — |
| v1.1 | ~1,255 test fns | — | — |
| v1.6 | ~3,056 test fns | — | no SSH library; system OpenSSH |

### Top Lessons (Verified Across Milestones)

1. Record live wire truth before writing code that depends on it. v1.1 and v1.6 both had captures overturn research and OpenAPI claims.
2. A guarded write is only as good as its refusal. Making the refusal the preview (v1.1) and the plan (v1.6) kept agents and humans on one reviewed artifact.
