---
quick_id: 261104-dnb
slug: pr35-round3-docs-sftp-read
branch: feat/deploy-tags-core
base: 1d55608
status: complete
---

# Quick 261104-dnb: PR #35 CodeRabbit round 3 (M1-M5) Summary

`read` on a Windows sftp host now probes the requested roots the listing
omitted, as `list_tree` does, so a held root is `transport_failed` instead of
an empty read; four doc sentences about the unmanaged plan status, residue and
the `--skip-gwbk` case of the late pair refusal are corrected.

## Commits

| Task | Commit | Subject |
|---|---|---|
| 1 | 4f0d489 | fix(261104-dnb): probe requested roots the Windows sftp listing omits in read (M5) |
| 2 | 78130f9 | docs(261104-dnb): name the unmanaged plan status, residue and the skip-gwbk case of the late refusal (M1-M4) |

## Per finding

- **M5 (fixed, TDD).** `crates/ignition-core/src/deploy/transport/sftp.rs`
  `read` computes `absent = chains - found.roots` (the same
  `BTreeSet<Vec<String>>` as `list_tree`) and passes it to `cross_check`. The
  `cross_check` doc now says both callers probe. Test
  `windows_read_refuses_a_requested_root_sftp_omits`: (a) a scope with a type
  the host lacks in every collection reads with no error (3 mode files);
  (b) the mode root hidden from its parent listing is `transport_failed` with
  "did not return" and `production/ignition/database-connection`; (c) the
  `local/ignition` folder hidden is `transport_failed` naming
  `local/ignition/database-connection`.
  RED: before the fix (b) read `mode: {}` with no error ("mode root omitted:
  ObservedFiles { ... mode: {} ... }"). Saved to the scratchpad
  `261104-dnb-t1-red.out`. After the fix: `cargo test -p ignition-core --lib
  sftp` 129 passed, T1-OK.
- **M1 (fixed).** README "Tags and redundant pairs": `deploy plan` lists an
  unmanaged tag unit with status `unmanaged`; only its pair-refusal warning
  does not name it.
- **M2 (fixed).** Same paragraph: "while such a file is there" is now "while a
  residue file of the kind refused above is there".
- **M3 (fixed).** README apply step 10: the run directory keeps its `.gwbk`
  (unless `--skip-gwbk`) and `result.json`; paragraph reflowed to the existing
  width.
- **M4 (fixed).** `crates/ignition-core/src/deploy/apply/mod.rs` branch table:
  the run directory and `result.json` stay, the `.gwbk` only when
  `--skip-gwbk` was not set.
- **Sweep.** README, apply/mod.rs and error.rs grepped for other `.gwbk` and
  `result.json` claims about the late refusal: README ~2711 already carries the
  `--skip-gwbk` qualifier; no other hit.

## Gate

- `cargo fmt --all -- --check`: FMT-OK
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (Homebrew
  rustc 1.94.1, first on PATH)
- `cargo test --workspace`: exit 0, 96 `test result: ok` lines, 3193 passed,
  0 failed; `wait_restart_witnessed_path_golden` passed first time (no flake).
  Log: scratchpad `261104-dnb-workspace.log`.
- `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets
  -- -D warnings`: clean, exit 0 (rustup stable rustc 1.99.1)
- FULL-GATE-OK

## Deviations

- Environment, not code: the first xwin clippy run failed with E0463 (can't
  find crate for `core`) because `/opt/homebrew/bin/cargo` comes first on PATH
  and the Homebrew toolchain has no Windows std. Rerun with
  `PATH="$HOME/.cargo/bin:$PATH"` (rustup stable, which has the
  `x86_64-pc-windows-msvc` target installed) was clean.

Noted, not changed: README apply step 10 still opens with "the
`.gwbk` is downloaded" without the `--skip-gwbk` qualifier; it is outside the
late-refusal claim the findings named.
