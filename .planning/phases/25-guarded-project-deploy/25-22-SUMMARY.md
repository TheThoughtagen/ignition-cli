---
phase: 25-guarded-project-deploy
plan: 22
subsystem: infra
tags: [release, windows, msvc, crt-static, github-actions, dumpbin]
status: complete
gap_closure: true
gap_ids: [G-25-7]

requires:
  - phase: 25-guarded-project-deploy
    provides: deploy workflows that download and run ign on a Windows runner
provides:
  - Windows release ign.exe links the C runtime statically (no VC++ redistributable needed)
  - Release-time dumpbin import check that blocks packaging of a dynamically linked ign.exe
  - Offline release-test.sh harness pinning flag placement and step order
  - RELEASING.md and docs/installation.md describing the five assets and the static CRT
affects: [25-27 live proof, release pipeline]

actuals:
  tokens: 7000
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Per-target RUSTFLAGS through setup-rust-toolchain's rustflags input (matrix key), never a .cargo/config.toml"
    - "Binary-format check with a vacuity guard (KERNEL32 must be present) before judging the list"

key-files:
  created:
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/release-test.sh
  modified:
    - .github/workflows/release.yml
    - RELEASING.md
    - docs/installation.md

key-decisions:
  - "Static CRT flag goes through the setup-rust-toolchain rustflags input (matrix.rustflags); a config-file flag would be overridden by the RUSTFLAGS the action exports"
  - "The four non-Windows entries carry rustflags '-D warnings' (the action's default), so their builds are unchanged"
  - "Import check requires KERNEL32.dll in the parsed list before judging, so a parse failure cannot pass vacuously"

requirements-completed: [DPRJ-01, DPRJ-05]
---

# Phase 25 Plan 22: Static CRT for the Windows release binary Summary

Windows release builds link the MSVC C runtime statically (`+crt-static` via the matrix `rustflags` input), and a dumpbin import check fails the job before packaging if ign.exe imports any vcruntime or msvcp DLL.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 1 (tracer) | Static CRT on the Windows target, import check before packaging, offline harness | 7c7269e |
| 2 | RELEASING.md and docs/installation.md | 1d8ac58 |

## Verification

- `actionlint .github/workflows/release.yml`: pass
- `bash .../harness/release-test.sh`: five PASS lines, exit 0 (run after each task)
- `grep -cF 'rustflags: ${{ matrix.rustflags }}'` = 1; `rustflags: '-D warnings'` = 4; `dumpbin` = 4; `GITHUB_STEP_SUMMARY` = 2
- `.cargo/config.toml` absent; `git diff --quiet HEAD -- .github/workflows/ci.yml Cargo.toml` exits 0
- Docs greps: `x86_64-pc-windows-msvc` in RELEASING.md = 2, `crt-static` = 1, `## The Windows binary` = 1, `there is no Windows build` = 0, `Windows x64` = 1, `redistributable` = 1

## Deviations from Plan

None - plan executed exactly as written. One note: the acceptance command `grep -c 'rustflags: ${{ matrix.rustflags }}'` without `-F` or an escaped `$` printed 0 in this shell; with `-F` or `\$` it prints 1. The harness uses `-F`.

## Not verified here

The live proof is plan 25-27's: the release run's import check passing on windows-latest, and `ign.exe --version` exiting 0 on ign-win-836 with the VC++ redistributable uninstalled. The pwsh step was checked only by actionlint (which runs shellcheck on bash, not pwsh), not executed.

## Known Stubs

None.

## Threat Flags

None. T-25-71 and T-25-72 mitigations are implemented as planned.

## Self-Check: PASSED

Files release.yml, release-test.sh (mode 100755), RELEASING.md, docs/installation.md exist; commits 7c7269e and 1d8ac58 exist.
