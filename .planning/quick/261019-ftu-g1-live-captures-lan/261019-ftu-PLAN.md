---
quick_id: 261019-ftu
slug: g1-live-captures-lan
type: docs
files_modified:
  - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
---

# Quick 261019-ftu: bring the G1 record in 23-LIVE-CAPTURES.md up to the `ls -lan` listing

## Goal

`## G1: POSIX sftp listing shapes` in 23-LIVE-CAPTURES.md describes the 23-22 design (bare-name `ls -la` lines read on POSIX) and names a test that has since been renamed. After debug session `.planning/debug/resolved/sftp-listing-parser-diff.md` (commits 206998e, 2219faf) the record must match the code.

## Task 1: update the G1 record (docs only)

1. Read the G1 section and its `### Rerun after the listing fix (2026-10-19)` subsection, and the resolved debug file (Resolution, Evidence 04:24Z-04:33Z, 11:07Z, 11:21Z, Known limitations KL1-KL3).
2. Mark the original G1 reading text superseded (a one-line note at the top of the section pointing to the new subsection). Keep the history text.
3. Add `### Superseded by \`ls -lan\` (2026-10-19, security review)` with, from the debug file only:
   - why: OpenSSH clients print raw `\n`/`\r` in names (utf8.c vasnmprintf); a bare line is byte-identical to a forged fragment; `str::lines()` dropped a trailing `\r`.
   - what: POSIX hosts listed with `@ls -lan` (`SftpOp::LsLongAllNumeric`); every POSIX entry line must carry the exact `<dir>/` prefix; output split on `\n` only, one `\r` stripped only when every line ends in CRLF (Win32-OpenSSH 9.5p2 measured uniform CRLF on ign-win-836); Windows keeps `@ls -la`.
   - evidence: live `lnxvm_listing_capture` on ign-lnx-836 with `-lan` (2026-10-19, 1 passed, home and production entries same as the bare capture); tests `posix_listing_refuses_captured_bare_openssh_8_9_output`, `posix_listing_parses_captured_ls_lan_output_from_openssh_8_9`, the six RED tests from c1c5781.
   - commits c1c5781, 206998e, 2219faf, ca9cd8f.
   - open: `lnxvm_cli_apply_rollback` has not been rerun on the `-lan` tree (last run before 206998e); KL1 apply-side Windows listings have no cross-check.
4. Replace any reference to a renamed test name in the G1 section with the current name.
5. Verify with grep that every test and gate name written exists in `crates/` (`grep -rn "fn <name>" crates/`). No key, token, password or IP address in the added text.

## Commit

One commit via `node ~/.claude/gsd-core/bin/gsd-tools.cjs query commit "docs(quick-261019-ftu): G1 live-captures record matches the ls -lan listing" --files .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md`. No co-author, no push. Do not commit .planning/config.json or .planning/research/.cache/.

## Done when

- The G1 section names only test names that exist, says POSIX listings use `ls -lan` with the prefix rule, keeps the superseded history, and records the open CLI-gate rerun.
