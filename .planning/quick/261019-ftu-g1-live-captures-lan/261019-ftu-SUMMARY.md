---
quick_id: 261019-ftu
slug: g1-live-captures-lan
status: complete
type: docs
completed: 2026-10-19
commit: 3a201b0
files_modified:
  - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
actuals:
  tasks: 1
  commits: 1
---

# Quick 261019-ftu: G1 record in 23-LIVE-CAPTURES.md matches the `ls -lan` listing

**The G1 section of 23-LIVE-CAPTURES.md now records that POSIX hosts are listed with `@ls -lan` under a mandatory `<dir>/` prefix rule, and no longer names a test that was renamed.**

## What changed (docs only)

- A one-line "Superseded" note at the top of `## G1: POSIX sftp listing shapes` points to the new subsection. The original 23-22 history (bare-name reading, Shape 1 and Shape 2 captures, first Linux gate) is kept.
- The renamed test reference is fixed: Shape 1 now cites `posix_listing_refuses_captured_bare_openssh_8_9_output` (the bare 8.9 capture is a must-refuse input); Shape 2 keeps `posix_listing_parses_captured_client_formatted_output`.
- New subsection `### Superseded by \`ls -lan\` (2026-10-19, security review)`, written from the resolved debug session only:
  - why: OpenSSH clients print raw `\n`, `\r`, `\t` in names; a bare line is byte-identical to a forged fragment; `str::lines()` dropped a trailing `\r`.
  - what: `SftpOp::LsLongAllNumeric` for POSIX, exact `<dir>/` prefix on every entry line, split on `\n` only with a single `\r` stripped only when every line is CRLF (Win32-OpenSSH 9.5p2 measured uniform CRLF), Windows keeps `@ls -la`.
  - evidence: live `lnxvm_listing_capture` with `-lan` (1 passed), the two named listing tests, the six RED tests from c1c5781.
  - commits c1c5781, 206998e, 2219faf, ca9cd8f.
  - open: `lnxvm_cli_apply_rollback` not rerun on the `-lan` tree; KL1 apply-side Windows listings have no cross-check.

## Verification

`grep -rn "fn <name>" crates/` finds every test and gate name written in the added text (12 names, all present). `SftpOp::LsLongAllNumeric` exists in `remote.rs`. The four commit hashes exist in `git log`. The added text holds no key, token, password or IP address.

## Deviations from Plan

None. The `### Rerun after the listing fix (2026-10-19)` subsection sits in the earlier 2026-10-18 capture area, outside the G1 section, so it was left unchanged as history.

## Commits

- 3a201b0: docs(quick-261019-ftu): G1 live-captures record matches the ls -lan listing

## Self-Check: PASSED
