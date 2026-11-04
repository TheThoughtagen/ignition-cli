---
quick_id: 261104-dnb
slug: pr35-round3-docs-sftp-read
branch: feat/deploy-tags-core
base: 1d55608
---

# Quick 261104-dnb: PR #35 CodeRabbit round 3 (M1-M5)

All five findings were checked against the code at 1d55608 and are valid.

## Task 1: M5, probe absent requested roots in sftp `read` (fix(261104-dnb))

- `crates/ignition-core/src/deploy/transport/sftp.rs` `read` (~1902) passes `&[]`
  as `absent` to `cross_check`. A requested root that the Windows sftp listing
  omits, but the host holds, is read as empty with no error.
- RED first: add a `#[tokio::test]` beside
  `windows_list_tree_refuses_a_type_folder_sftp_omits` using the same fake
  (`windows_fake`, `windows_tree`, `knobs().hidden`) that calls `read` with a
  scope whose root sftp hides, and asserts `transport_failed` with "did not
  return" and the root path. Also assert a requested root the host lacks too is
  still no error (existing read tests must stay green). Record RED in the
  scratchpad file `261104-dnb-t1-red.out`.
- Fix: compute `absent` in `read` exactly as `list_tree` does
  (`chains.iter().filter(|c| !found.roots.contains(c))`) and pass it. Update the
  `cross_check` doc comment ("`read` passes none") to say both callers probe.
- Check `chains` in `read` is the same type (`BTreeSet<Vec<String>>`) as in
  `list_tree`; if absence of a scoped root is legitimately normal for some read
  scope, the probe only errors when the host holds the folder, so that stays OK.
- Gate: `cargo test -p ignition-core --lib sftp` green; print T1-OK.

## Task 2: M1-M4 docs (docs(261104-dnb))

- M1 README ~2692: "`deploy plan` does not name it" is wrong. `deploy plan`
  lists the unit with status `unmanaged`; only the pair-refusal warning omits
  it. Reword to say that.
- M2 README ~2698: "while such a file is there" is ambiguous after the
  unmanaged sentence. Say "while a residue file of the kind listed above is
  there" (or equivalent naming residue).
- M3 README ~3764 (apply step 10): "keeps its `.gwbk` and `result.json`" ->
  "keeps its `.gwbk` (unless `--skip-gwbk`) and `result.json`". Keep line width.
- M4 `crates/ignition-core/src/deploy/apply/mod.rs:75` branch table row: the
  `.gwbk` stays only when `--skip-gwbk` was not set.
- grep README, apply/mod.rs, error.rs for any other "`.gwbk` and `result.json`"
  claim about the late refusal and apply the same qualifier.
- Then the full gate: cargo fmt --check; cargo clippy --workspace --all-targets
  -- -D warnings; cargo test --workspace; cargo xwin clippy --target
  x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings. Print
  FULL-GATE-OK. Write 261104-dnb-SUMMARY.md (not committed).

## Constraints

No slug, exit-code, Cargo version or golden changes. No prod calls, no
worktrees, no co-author lines, no host names or IPs. Never stage
.planning/config.json, .planning/research/.cache/,
svc-ignition-deploy@52.154.75ace-uat4.ps1, .planning/debug/tg-pg-table-check.md.
Do not commit PLAN, SUMMARY or STATE. Do not push or reply on the PR.
