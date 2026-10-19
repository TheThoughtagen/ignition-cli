---
phase: 23-guarded-apply-rollback
scope: UAT gap closure (23-22 G1, 23-23 G2, 23-24, debug sftp-listing-parser-diff), git diff 5fa3897..HEAD -- crates/
reviewed: 2026-10-19T13:53:21Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/tests/live_transport_lnxvm.rs
  - crates/ignition-core/tests/live_transport_wflwvm01.rs
  - crates/ignition-core/tests/live_deploy_apply.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
findings:
  critical: 0
  warning: 2
  info: 8
  total: 10
status: issues_found
---

# Phase 23: Code Review Report (UAT gap closure)

**Reviewed:** 2026-10-19T13:53:21Z
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Scope: the crates/ diff from 5fa3897 to HEAD. That covers G1 (POSIX sftp listing, c67e8bc), G2 (`survey_root` from an 8.3 short-name home, f0dced9, plus the mkdir error text, 3a47279) and the sftp-listing-parser-diff fix (`ls -lan` with a required prefix and `output_lines`, 206998e). Context came from 23-22/23-23/23-24-SUMMARY.md and `.planning/debug/resolved/sftp-listing-parser-diff.md`.

**Listing parser.** I found no input that the parser misreads when it should refuse it.

- POSIX: every POSIX entry line must carry the exact `<dir>/` prefix with a space before it and one `/`-free component after it.
  - A POSIX name cannot hold `/`. So the line after a raw `\n` can never carry the prefix and is always refused.
  - The last `/` on a valid line is the one before the name, so `rfind` cannot land on a different occurrence.
  - Nested listed dirs give a name holding `/`, which is refused.
  - An injected `Remote working directory:` line always adds one marker too many.
  - `x\n` at the end of a listing becomes an empty line, which is refused.
  - A `\n`-only name yields an empty name, which is refused.
- Uniform-CRLF stripping (D2): an attacker cannot trigger it through an LF client. Every batch holds at least one `@pwd` marker line, and that line ends in the home. The KL3 home is outside the attacker model.
- Stdout overflow fails the run (`OVERFLOW_CODE`), so the cap cannot truncate a listing into a short name.
- Both the `-n` client-formatting claim and the GLOB_MARK/`path_append` prefix shape match openssh-portable `do_ls_dir`.

The remaining parser items are defence-in-depth (IN-01, IN-02).

**survey_root.** The home-prefix test compares whole components, so `/home/u` never matches `/home/u2`. It handles the `prepare` path correctly, because that path builds its root from the cached home.

The two warnings are in the code around it:
- **WR-01:** the phase-22 mirror `stage` path almost never has the home cached in production. For that path the G2 fix and the "nothing above the home is listed" property are silently off. The unit test that claims coverage seeds the home through the `with_home` test seam.
- **WR-02:** the home match is case-sensitive and does not handle 8.3 spellings. On a Windows host, a root spelled differently falls back to the drive-root survey that G2 was about.

**Error text.** The mkdir classification is only diagnostic text, but it depends on the client's wording (IN-03).

**Other changes.** The `main.rs` change (an `IsTerminal` import kept for headless builds) is correct: line 3267 uses it outside the `tui` feature gate. The `deploy_family_coverage.rs` CRLF changes are correct. I only skimmed `live_deploy_apply.rs` (+530 lines, the ignored C2b queued-config-save rig gate). It is outside the listing and survey scope, and I report nothing on it.

## Warnings

### WR-01: The mirror `stage` path surveys from the home only when something else already cached the home, which production code never does

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:1961-1975`, `crates/ignition-core/src/deploy/transport/sftp.rs:2141-2149`, `crates/ignition-core/src/deploy/transport/mod.rs:401-426`, `crates/ignition-core/src/deploy/transport/mod.rs:540-546`
**Issue:** `survey_root` only reads `self.home.get()` and opens no session. `stage_inner` calls it without ever calling `account_home()`.

The public pairing `default_staging_mode` + `mirror_for_node` never fills that cache:
- `default_staging_mode` builds a throwaway `sftp_transport(...)`, calls `default_staging` on it and drops it. The home is cached on that instance only.
- `mirror_for_node` builds a fresh `SftpTransport` with an empty `OnceLock`.

So for the documented API, `stage` always falls back to `/<first component>`:
- On a Windows runner whose home is `C:\Users\RUNNER~1`, the survey lists `/C:/Users`, does not find `RUNNER~1`, and issues `mkdir "/C:/Users/RUNNER~1"`. That is exactly G2.
- On POSIX it lists `/home`, which contradicts the comment at 2144-2146 ("the account home (listed first, so an unreadable home is `transport_failed`)").

The behaviour also depends on the order of earlier calls on the same instance: the same `stage` surveys differently depending on whether something called `account_home` first.

`mirror_stage_below_the_home_surveys_from_the_home` (sftp.rs:3957) passes only because it calls `.with_home(HOME)`, a `#[doc(hidden)]` test seam. The fallback is already visible in the unchanged `prepare_over_a_bare_name_posix_server` test (sftp.rs:3763), which has to pre-list `/home` because no home is cached.

23-23-SUMMARY says "`stage_inner` (the phase-22 mirror staging) uses it". That is true only when the home is cached.

The CLI does not call `mirror_for_node` today (it uses `prepare`/`put_tree`, which goes through `staging_abs(account_home())`), so this is not a shipping blocker. It is still a public `ignition-core` API whose G2 fix does not take effect.

**Fix:** have the survey path load the home instead of peeking at the cache. Make `survey_root` async, or prime the cache in `stage_inner` before calling it:
```rust
StagingMode::Separate { root } => {
    // the survey base is the home when the root lies below it (G2); ask once
    let _ = self.account_home().await?;
    let (base, rest) = self.survey_root(root).map_err(|why| self.mirror_fail(OP, why))?;
```
Then drop `.with_home(HOME)` from `mirror_stage_below_the_home_surveys_from_the_home` so the test exercises the real `@pwd` path. Add a Windows variant in which the fake's `@pwd` answers with the 8.3 alias.

### WR-02: The home match in `survey_root` is exact and case-sensitive, so on a Windows host a root spelled with another case or the other 8.3/long form falls back to the drive-root survey

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:1966-1973`
**Issue:** `parts[..home_parts.len()] == home_parts[..]` compares component strings byte for byte.

On a Windows host the following all name the same directory, but none of them match each other:
- `/C:/Users/svc/...` vs `/c:/users/svc/...` (case)
- `/C:/Users/RUNNER~1/...` vs `/C:/Users/runneradmin/...` (8.3 vs long)

Any non-matching root silently takes the `/<first component>` branch. That branch lists the drive root and its children, and it issues `mkdir` for the home's own spelling when that spelling is the alias: this is the G2 failure, now reported with the new "may already exist" text.

`validate_staging_root_for` already treats Windows paths as case-insensitive (mod.rs:710-713), so the two checks disagree about which root is "below the home". Today `prepare` builds its root from the home string, so only explicit `StagingMode::Separate` roots reach this branch (the mirror path, and any future `--staging` flag). Nothing tells the operator that the home-based survey was skipped.

**Fix:**
1. Compare home components ASCII-case-insensitively when `self.windows`, keeping `home` (the server's spelling) as `base`.
2. For an explicit root that does not match but shares the home's parent, refuse with a clear message rather than surveying from the drive root. An 8.3 vs long mismatch cannot be resolved offline.

```rust
let same = |a: &String, b: &String| if self.windows { a.eq_ignore_ascii_case(b) } else { a == b };
&& parts.iter().zip(&home_parts).all(|(a, b)| same(a, b))
```

## Info

### IN-01: A POSIX prefixed line is accepted with more than eight columns before the name

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:416`, `crates/ignition-core/src/deploy/transport/sftp.rs:528-532`
**Issue:** For `ListFormat::Posix`, the client always formats `ls -lan` lines as exactly eight columns (mode, `?`/links, uid, gid, size, month, day, time-or-year). The parser only checks `columns.len() < 8`, then takes the size from `len - 4` and the date from the last two columns. I found no reachable input that exploits the extra slack, because a name fragment cannot carry the prefix. Still, the D1 anchor is the strongest rule the parser has, and an exact column count would make any unexpected line shape fail closed.
**Fix:** For `ListFormat::Posix`, require `columns.len() == 8`.

### IN-02: A final line without a trailing `\n` is parsed as a complete entry

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:505-514`
**Issue:** `split_terminator('\n')` yields an unterminated last fragment as a normal line. The OpenSSH client always ends a listing line with `\n`, and overflow and timeout are already failures. Even so, a listing that ends mid-line (a client killed after a partial write but reporting exit 0, or a future runner change) would read a truncated name as a real entry instead of refusing it.
**Fix:** In `parse_batch_listing_as` and `account_home`, refuse non-empty stdout that does not end in `\n`.

### IN-03: The mkdir error text depends on the client's wording and scans every stderr line

**File:** `crates/ignition-core/src/deploy/transport/remote.rs:629-638`, `crates/ignition-core/src/deploy/transport/remote.rs:1006-1010`
**Issue:** `failing_command_is_mkdir` matches `remote mkdir "<path>": Failure`, which is the wording of recent OpenSSH clients. My recollection is that older portable releases word it `Couldn't create directory: Failure`; I have not checked this against the source. If that is right, a runner with such a client gets the old "a held or non-empty target" text, which is the misleading message 3a47279 set out to remove.

The caller already knows the batch is mkdir-only: `make_dirs` runs batches of `SftpOp::Mkdir` and nothing else. Classifying by stderr text is therefore the weaker signal.
**Fix:** Map the failure at the `make_dirs` call site, where the batch is known to be mkdir-only (for example a `require_success_for(op_kind, ..)` variant or a detail rewrite in `make_dirs`). Keep the text match only as a fallback.

### IN-04: Survey children are matched case-sensitively on a Windows host

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:2032`
**Issue:** `listings[index].iter().find(|e| e.name == *name)`. On NTFS, an existing `IGN-DEPLOY` or a gateway or mode directory that differs in case reads as `Missing`, and the following `mkdir` fails with `Failure`. This was already true before the gap closure, but the survey now starts at the home, so these are the first names looked up. The new text then points the operator at 8.3 spellings, which is the wrong cause.
**Fix:** When `self.windows`, match with `eq_ignore_ascii_case`. The parser already guarantees that no two names in a Windows listing differ only by case, so the match is unambiguous.

### IN-05: The home itself and its ancestors are no longer checked for links

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:1950-1975`
**Issue:** The old survey from `/<first>` refused a symlink at any component of the staging root, including the home and its ancestors. The new base is the home, which is listed (its contents are followed) but never classified.

On POSIX this is sound, because the `@pwd` answer is the server's `realpath(".")` and contains no symlink components. On Win32-OpenSSH, a home that is a junction is followed without comment (and a junction already lists as a plain directory, C7a-5).

The 23-23 threat notes (T-23-88) cover components below the home only.
**Fix:** Record in the `survey_root` doc comment that the home is trusted as the server reported it (realpath on POSIX), so the relaxation is an explicit decision.

### IN-06: The live Linux harness reads the home with the `str::lines()` pattern the debug session banned

**File:** `crates/ignition-core/tests/live_transport_lnxvm.rs:409-422`
**Issue:** `home_sftp` uses `text.lines().find_map(...)`. That strips a trailing `\r`, ignores extra lines, and takes the first marker. The result feeds `removable()` and every `rm -rf` guard root in the harness. The account home is administrator-set, so this is not exploitable here, but the harness now disagrees with `SftpTransport::account_home`, which refuses extra lines.
**Fix:** Call `SftpTransport::account_home()` on a transport over the same shell, or require exactly one line, split on `\n` only.

### IN-07: The POSIX "nothing above the home" tests use a string prefix, not a component boundary

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:3900`, `crates/ignition-core/src/deploy/transport/sftp.rs:3984`
**Issue:** `dirs.iter().all(|d| d.starts_with(HOME))` would also pass for `/home/svc2/...` or `/home/svcX`. The Windows counterpart (around sftp.rs:3866) already uses `dir == short || dir.starts_with(&format!("{short}/"))`.
**Fix:** Use the same component-boundary check in both POSIX tests.

### IN-08: `wflwvm01_short_name_home_prepare` skips cleanup on most failure paths and does not exercise a short-name `@pwd`

**File:** `crates/ignition-core/tests/live_transport_wflwvm01.rs:2334`, `crates/ignition-core/tests/live_transport_wflwvm01.rs:2372-2376`, `crates/ignition-core/tests/live_transport_wflwvm01.rs:2398-2415`
**Issue:**
- Only the `prepare` error branch calls `w.cleanup()`. Each of the following panics with the scratch tree, the staging directory and the deploy root left on the live host:
  - the `assert_ne!` on the short spelling, which runs after `WinScratch::new` has already written
  - `commit(..).expect`
  - the post-commit asserts
  - `restore(..).expect`
- The short home is injected through `with_home(...)`, so the live gate never sees a server whose `@pwd` answers with an 8.3 name, which is the real G2 trigger. That is left to the windows-latest CI leg, so the gate proves less than its name suggests.

**Fix:** Run the body in a closure or `async` block, call `w.cleanup().await` on every exit (a guard or a `match` over the result), and say in the doc comment that the `@pwd` short-name path is covered by CI only.

## Structural notes (no fallow input provided)

Checked:
- `apply_ops.rs` no longer imports `absolute_components`, which is now used only in `sftp.rs` (`survey_root`, `with_mirror`). No dead import is left.
- `put_tree` and `staging_abs` were widened to `pub(super)` for the unit tests. They are not reachable outside `transport::sftp`.

---

_Reviewed: 2026-10-19T13:53:21Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
