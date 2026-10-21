---
status: resolved
trigger: "Background security review of pushed commits flagged a parser-differential in crates/ignition-core/src/deploy/transport/sftp.rs (no details given)."
created: 2026-10-19
updated: 2026-10-19T13:45Z
resolved: 2026-10-19T13:45Z
goal: find_and_fix
---

# Debug: sftp listing parser differential

## Symptoms

DATA_START
- expected: Every sftp directory listing the engine parses either matches the real directory exactly or is refused. An ambiguous line is never misread as a different name, an extra entry or a directory marker.
- actual: Unconfirmed. A background security review flagged a "parser-differential" in sftp.rs with no detail. Orchestrator read of `parse_batch_listing_as` / `line_columns_and_name` (G1 change c67e8bc, plan 23-22) gives two candidates:
  - F1: a POSIX filename containing a newline (for example `x\n-rw-r--r-- 1 root root 5 Oct 18 22:52 fake`, or `x\nRemote working directory: ...`) splits into lines that `text.lines()` reads as extra entries or as a marker, so the listing could disagree with the directory instead of being refused.
  - F2: a symlink line whose `-> target` text holds the listed directory prefix. `line.rfind(prefix)` would take the target's component as the entry name, if a client prints link targets.
- errors: none observed; this is a security review finding.
- timeline: introduced with c67e8bc (23-22, bare-name POSIX listings). Pushed to branch phase-23-ci (draft PR #21), not main.
- reproduction: unknown until confirmed. Must check what real OpenSSH sftp clients print for such names: macOS client, Ubuntu 22.04 OpenSSH 8.9p1 sftp-server on ign-lnx-836 (keys/env in ~/.cache/ign-lnxvm01 and scratchpad linuxhost/env.sh; az vm start it if deallocated, approved test VM), Win32-OpenSSH (ign-win-836, ~/.cache/ign-wflwvm01). Check whether the client escapes nonprintables and whether `ls -la` shows `-> target`.
DATA_END

## Constraints

- Write a RED test for each case that reproduces, then fix so any ambiguous listing is refused, never misread.
- No push, no co-author trailer, one commit per step, no worktree, never print secrets/keys/tokens, use /bin/rm (shell rm is wrapped).
- Do not touch ~/.config/ignition-cli or the Keychain; wrap any `ign` in `timeout 120 ... </dev/null`.
- Do not commit .planning/config.json or .planning/research/.cache/.
- If the permission system denies an action, stop and report it; do not work around it.
- After the fix: cargo test --workspace, clippy -D warnings, fmt --check, `cargo build -p ignition-cli --no-default-features`. Run long ones in the background.

## Current Focus

hypothesis: CONFIRMED. (A) A POSIX name holding a raw \n is printed by every OpenSSH client as two physical lines, and since c67e8bc the parser reads a line without the listed-directory prefix as a bare server longname, so the fragment after the \n is accepted as a forged entry (any name/mode/size) and the real entry is hidden. (B) `text.lines()` strips a \r before \n, so a name ending in \r is read without it and passes safe_remote_path.
bug_class: Bohrbug (deterministic text parse).
known_pattern_candidate: none (no knowledge-base.md yet).
status_note: RESOLVED. RED c1c5781, fix 206998e, harness 2219faf, README ca9cd8f; full gauntlet green (Evidence 13:40Z). Not pushed.

reasoning_checkpoint:
  hypothesis: "parse_batch_listing_as misreads a listing because (A) the OpenSSH client prints \n inside a name raw AND line_columns_and_name falls back to a bare read for any line without the `<dir>/` prefix, and (B) the client prints \r raw AND str::lines() strips a \r before \n"
  confirming_evidence:
    - "Real parser over verbatim OpenSSH_10.3p1 captures: bare, client-formatted and -lan shapes all ACCEPTED as {fake, plain, x} for a directory holding {plain, x\\n<forged line>}; `y\\r` ACCEPTED as `y` (Refuse and Skip)"
    - "openssh-portable 8.9 utf8.c vasnmprintf copies \\n, \\r, \\t raw when wp == NULL (mprintf)"
    - "c67e8bc^ refused prefix-less lines; c67e8bc added the split_columns fallback"
  falsification_test: "If the parser, fed the captured bytes, had refused bare_nl/pref_nl/lan_nl or read `y\\r` as unsafe, the hypothesis would be wrong. It did not."
  fix_rationale: "Bare `ls -la` lines are byte-identical to forgeries, so no parser rule can separate them. `@ls -lan` makes every OpenSSH client format every line itself with the absolute `<dir>/` prefix (do_ls_dir LS_NUMERIC_VIEW branch); a fragment after a raw \\n cannot hold `/`, so requiring the prefix on every POSIX entry line refuses every split. Splitting on \\n only keeps a trailing \\r in the name, where safe_remote_path refuses it (or NameRule::Skip skips it)."
  blind_spots: "(1) Whether the Win32-OpenSSH sftp.exe client (windows-latest CI, Windows operators) writes CRLF to a pipe: unverified; a plain \\n split would then leave \\r on every line. (2) The live Ubuntu 8.9 host has not been listed with -lan (shape proven from the local client + 8.9 source; the shape is client-determined). (3) Windows hosts keep bare `ls -la` lines; Win32 names cannot hold \\n/\\r and Windows reads are PowerShell cross-checked, but apply-side Windows listings are not."
  candidate_causes:
    - "code: per-line either-shape read in line_columns_and_name (c67e8bc)"
    - "code: str::lines() CR stripping in parse_batch_listing_as (pre-existing)"
    - "environment/tooling: OpenSSH mprintf passes \\n/\\r/\\t raw; bare server longname carries no anchor"
    - "data: POSIX names may hold any byte but `/` and NUL"
  and_gate: "yes -- (A) needs the raw-\\n client AND the unanchored bare read; (B) needs the raw-\\r client AND lines() stripping. Fixing the parser side of each is sufficient because the client side is not ours."

tdd_plan (RED first, sftp.rs unit tests, captures normalised to a fixed dir such as /d/nl):
  - posix listing refuses a name split by a raw newline: the -lan capture (marker, prefixed `.`, `..`, `plain`, `x`, then the prefix-less forged line) -> Err(Bad), not {fake, plain, x}; same for the bare and client-formatted captures; both NameRule values.
  - posix listing refuses a name ending in a carriage return: `.../y\r\n` -> Err(Bad) under Refuse, entry left out (not `y`) under Skip.
  - list_level emits `@ls -lan "<dir>"` for a POSIX transport (batch-line assertion).
  - boundary neighbours: `x\n` (trailing newline -> empty line), `\nfake` (empty name after the prefix), `cr\rmid` (already refused), CRLF-uniform output if that rule is adopted.
  - existing tests to port: posix_listing_parses_captured_openssh_8_9_output / _client_formatted_output / _takes_either_shape_per_line / bare-owner test, FakeSftp `bare_posix` knob and every `@ls -la "` matcher (3051, 3279, 3347, 3518, 3682), live_transport_lnxvm.rs:768-770.
decisions (session manager, 2026-10-19, from the standing "refuse, never misread" constraint):
  - D1: POSIX hosts are listed with `@ls -lan "<dir>"` and EVERY ListFormat::Posix entry line must carry the exact `<dir>/` prefix; no bare fallback. This REVERSES the 23-22 decision ("ls -lan is not used: it would force one shape on OpenSSH clients and leave a captured 8.9 listing unreadable", ListFormat doc comment, c67e8bc). Reason: a bare `ls -la` line is byte-identical to a fragment forged by a raw \n in a name, so no parser rule can tell them apart; `-n` makes every OpenSSH client format every line itself with the absolute prefix, and a fragment cannot hold `/`. The captured bare 8.9 listing becomes a must-refuse input for POSIX. Windows hosts keep bare `@ls -la`.
  - D2: split stdout on \n only; strip exactly one trailing \r per line only when EVERY line (client-generated `Remote working directory:` markers included) ends in \r\n. Otherwise a \r stays in the name and is refused (Refuse) or left out (Skip). Applies to both ListFormats.
  - D3: before GREEN lands, re-run the read-only `lnxvm_listing_capture` live gate with `-lan` against ign-lnx-836 (approved test VM; az vm start if deallocated). GREEN-phase task, not RED.
known_limitations:
  - KL1 (apply-side Windows): Windows hosts keep bare `ls -la` lines, which carry no anchor. Win32 names cannot hold \n or \r and Windows READS are cross-checked by the PowerShell enumeration, but apply-side Windows listings (staging survey, prepare) have no cross-check. Accepted as a known limitation, not fixed here.
  - KL2 (Win32 client line ends): RESOLVED 2026-10-19 by measurement on ign-win-836 -- OpenSSH_9.5p2 for Windows writes uniform CRLF to a pipe (and to a file) for markers, bare `-la` lines and client-formatted `-lan` lines alike (Evidence 11:07Z). D2 stands as written. Not measured: the windows-latest runner's own sftp.exe build (the CI leg will exercise it when phase-23-ci is pushed) and a Windows client against a POSIX host (same client stdout path, argued in Evidence 11:40Z).
  - KL3 (account_home trailing CR, LF client): `@pwd` alone prints one line, so a POSIX home whose last component ends in `\r`, read through an LF client, is a single line ending in `\r\n` and is indistinguishable from a CRLF client's line; D2 strips it and the home is read without its `\r` (same as before this fix, not a regression). Not fixed: the home is set by the host administrator (root), outside the threat model (names an attacker can create below the data dir or home); a disambiguating extra line would need a new batch op. A home split by a raw `\n` IS now refused.

tdd_checkpoint:
  test_file: "crates/ignition-core/src/deploy/transport/sftp.rs (mod tests, section `a raw newline or carriage return inside a listed name`)"
  test_names:
    - posix_listing_refuses_a_name_split_by_a_raw_newline
    - posix_listing_refuses_a_name_ending_in_a_carriage_return
    - posix_listing_refuses_names_next_to_a_raw_newline
    - posix_listing_strips_a_carriage_return_only_when_every_line_ends_in_crlf
    - windows_listing_strips_a_carriage_return_only_when_every_line_ends_in_crlf
    - a_posix_host_is_listed_with_ls_lan_and_a_windows_host_with_ls_la
  status: "green (six RED tests pass on 206998e; RED block byte-identical to c1c5781)"
  red_commit: "c1c5781 test(23): failing rows for a raw newline or carriage return in an sftp listing name (sftp.rs only, not pushed)"
  failure_output: "6 failed, 0 passed: every case is an assertion failure that shows the forged `fake` / misread `y` / `@ls -la` the current code accepts (see Evidence 2026-10-19T05:10Z)"
  green_must_port (these assert the old behaviour and will break under D1/D2): posix_listing_parses_captured_openssh_8_9_output (bare 8.9 capture becomes must-refuse for Posix), posix_listing_takes_either_shape_per_line, posix_bare_listing_with_a_spaced_owner_is_refused_not_misread (its Ok controls are bare), parse_ls_listing_cases `dots` (bare `.`/`..`), read_over_a_bare_name_posix_server + prepare_over_a_bare_name_posix_server (bare_posix knob), FakeSftp `@ls -la "` matcher at the `if l.starts_with("@ls -la \"")` branch and listed_dirs/session filters (`@ls -la "` at ~3051/3279/3347/3518/3682 before this commit's +~330 line shift), live_transport_lnxvm.rs:768-770, the ListFormat doc comment (the 23-22 "ls -lan is not used" text), SftpOp::LsLongAll doc/render or a new op variant.
green_phase (2026-10-19, continuation agent): RED confirmed by the session manager (6 failed at sftp.rs 2716/2756/2796/2862/2897/2922). KL2 RESOLVED by measurement (Evidence 11:07Z): Win32-OpenSSH 9.5p2 sftp.exe writes uniform CRLF to a pipe; D2 stands as written.
next_action: none -- resolved 2026-10-19T13:45Z. Fix committed (206998e fix, 2219faf live gate harness, ca9cd8f README), not pushed. Follow-up outside this session: the windows-latest CI leg exercises the runner's own sftp.exe (KL2 residual) when phase-23-ci is next pushed.

## Evidence

- timestamp: 2026-10-19T04:20Z
  checked: git status / ancestry
  found: checkout is on main (ahead of origin/main by 166); c67e8bc IS an ancestor of HEAD; main = origin/phase-23-ci + one docs commit (c4d1d90). The affected code is in this checkout.
  implication: work on main, no branch switch needed.

- timestamp: 2026-10-19T04:21Z
  checked: sftp.rs parse_batch_listing_as / line_columns_and_name / split_columns; remote.rs safe_remote_path; process.rs utf8_locale; list_level_as; descend_from; account_home
  found: the parser splits stdout with `str::lines()` (splits on \n, strips one trailing \r). A line is read either as `<dir>/<name>` (rfind of the exact prefix) or bare (text after the eighth column). safe_remote_path refuses control chars (incl. \n, \r, \t), `"`, `\`, glob chars. The client runs with LC_ALL=en_US.UTF-8 (macOS) / C.UTF-8. POSIX reads have no independent cross-check (the PowerShell cross_check is Windows only). descend_from trusts `entries.find(name)`.
  implication: any raw \n inside a name splits into a line the parser parses on its own; a trailing \r is stripped before the name check.

- timestamp: 2026-10-19T04:24Z
  checked: local OpenSSH_10.3p1 client (`sftp -q -b - -D /usr/libexec/sftp-server`, LC_ALL=en_US.UTF-8) over files named `x\n-rw-r--r--    1 root     root            5 Oct 18 22:52 fake`, `y\r`, `t\tab`, `cr\rmid`, `nel\u0085x`, `u v`, `z\nRemote working directory: elsewhere`; bare shape via `sftp-server -P users-groups-by-id`, client-formatted shape via the default server (captures in scratchpad f1/ and f1b/)
  found: the client prints \n, \r and \t inside a name RAW in both shapes (only C1 U+0085 was escaped, as `\302\205`). Bare shape prints `... 04:26 x` then a separate line `-rw-r--r--    1 root     root            5 Oct 18 22:52 fake`; `y\r` prints as `... y\r\n`. The injected `Remote working directory: elsewhere` appears as its own line.
  implication: F1 is real at the client level: an OpenSSH client hands the parser a forged, well-formed bare line and a CR-terminated name. Escaping does not protect us.

- timestamp: 2026-10-19T04:26Z
  checked: same client with `@ls -lan` against the bare-shape server (scratchpad f1b/lan_*.bin)
  found: with -n the client formats every line itself: every real entry (incl. `.`/`..`) carries the absolute `<dir>/` prefix and numeric uid/gid; the forged fragment line after the raw \n carries NO prefix (a name fragment cannot hold `/`). `y\r` still prints raw as `.../cr/y\r\n`.
  implication: a prefix-required parse over `ls -lan` output can detect every raw-\n split (the line after it has no prefix); the trailing-\r case needs \n-only splitting on its own.

- timestamp: 2026-10-19T04:28Z
  checked: openssh-portable V_8_9_P1 utf8.c vasnmprintf (fetched to scratchpad osrc/), the routine behind mprintf
  found: `else if (wp == NULL && (wc == L'\n' || wc == L'\r' || wc == L'\t')) { width = 0; }` -> copied raw; every other non-printable is escaped `\ooo`. mprintf passes wp == NULL.
  implication: raw \n/\r/\t in names is documented OpenSSH client behaviour (8.9 through 10.3), not a macOS quirk. Every client the engine runs (macOS, the Ubuntu CI client, Win32-OpenSSH built from portable) does it.

- timestamp: 2026-10-19T04:29Z
  checked: the REAL parser (`parse_batch_listing_as(Posix, Refuse|Skip, ..)`) over the verbatim captures, via a throwaway integration test (crates/ignition-core/tests/zz_tmp_f1_probe.rs, deleted after the run; tree clean)
  found: bare_nl, pref_nl and lan_nl -> ACCEPTED [("fake", File, 5), ("plain", File, 2), ("x", File, 2)] while the directory holds two entries (`plain`, `x\n-rw-r--r--    1 root     root            5 Oct 18 22:52 fake`). bare_cr, pref_cr, lan_cr -> ACCEPTED [("y", File, 2)] while the only entry is `y\r`. Same under NameRule::Skip. bare_mk and a two-directory batch with an injected `Remote working directory:` line -> REFUSED ("more directory markers than directories").
  implication: F1 CONFIRMED in every shape: the parser invents `fake` (any mode, size, kind the name spells) and `x`, and hides the real entry. CR variant CONFIRMED: `y\r` is misread as `y`. Marker injection is refused by the marker count.

- timestamp: 2026-10-19T04:31Z
  checked: c67e8bc^ (phase-22 `ListFormat::Prefixed`) parser
  found: the old POSIX path refused any line without the listed-directory prefix ("a listing line does not carry a listed directory prefix"). c67e8bc replaced it with a per-line either-shape read: a line without the prefix falls through to `split_columns(line, 8)` and is read as a bare server longname.
  implication: for client-formatted (macOS-server) listings the \n forgery is a regression from c67e8bc; for bare servers (Ubuntu 8.9, the G1 target) there is no anchor at all. The \r stripping (`text.lines()`) predates c67e8bc.

- timestamp: 2026-10-19T04:32Z
  checked: F2 -- symlink listings from the same client (bare and client-formatted, scratchpad f1b/bare_ln.bin, pref_ln.bin) and the parser over synthetic `-> target` lines
  found: OpenSSH prints no `-> target` in either shape (0 occurrences; server longname and client ls_file never readlink). Synthetic `... 22:52 /d/sub/link -> /d/sub/target` and `... 22:52 link -> /d/sub/target` are REFUSED by the date check (the columns before the rfind hit end in `link ->`). A synthetic `... link -> target` (no prefix in the target) reads as one Symlink named `link -> target`, and symlinks are refused downstream.
  implication: F2 is not reachable with OpenSSH and is refused when the target holds the prefix.

- timestamp: 2026-10-19T04:33Z
  checked: whether a parser-only defence exists for bare listings (ordering, counts)
  found: the forged line is byte-identical to a real server longname. The client sorts by strcmp, but a forger picks a name that keeps the order (fragment `x`, forged `xz`, next real `y`). `.`'s link count counts subdirectories only. No other column carries an entry count.
  implication: bare `ls -la` output cannot be made unambiguous in the parser; the listing command has to anchor every line. openssh-portable 8.9 sftp.c do_ls_dir: with LS_NUMERIC_VIEW (`-n`) the client always formats the line itself as `ls_file(fname, ..)`, fname = path_append(<absolute listed path>, name) (strip_path is NULL for an absolute path, I_LS case), numeric uid/gid, same eight columns (sftp-common.c ls_file remote=1).

- timestamp: 2026-10-19T05:10Z
  checked: TDD RED. Six tests added to sftp.rs mod tests (captures f1b/{bare,pref,lan}_{nl,cr}.bin normalised to /d/nl, /d/cr, pwd /home/u, byte-exact otherwise); `cargo test -p ignition-core --lib -- <the six names>`; clippy -p ignition-core --all-targets -D warnings clean; fmt --check clean
  found: 6 failed / 0 passed, all assertion failures (no compile error, no panic elsewhere). Newline split: bare, client-formatted and ls -lan x Refuse/Skip all Ok([fake(File,5), plain, x]). CR: bare/client/lan Refuse Ok([y]); client/lan Skip Ok([y]). Neighbours: `x\r\n<forged>` Ok([fake, x]) under both rules; `x\n` (middle and last), `\nfake`, `\n<forged>` already refused (guards). POSIX line ends: LF marker + CRLF entries Ok([plain, x]) under Refuse and Skip; CRLF listing of `x\n<forged>` Ok([fake, plain, x]) under both; the LF-uniform, CRLF-uniform and CRLF-uniform `y\r` controls already pass (guards). Windows: LF marker + CRLF entries Ok([a.json, sub]); LF/CRLF-uniform controls pass. Batch line: Windows `@ls -la "/C:/gw/data"` passes (guard); POSIX session is [["@pwd", "@ls -la \"/gw/data\""]] where `@ls -lan` is required.
  implication: the defects reproduce as unit tests in every shape; the guards pin the boundary neighbours the fix must not regress (trailing-empty-segment handling of the \n-only split, the empty-name check, CRLF-uniform stripping).

- timestamp: 2026-10-19T11:05Z
  checked: KL2 pairings -- .github/workflows/ci.yml, tests/common/sftp_server.rs, tests/deploy_host_conformance.rs, remote.rs (sftp_argv, render_on, restrict_to_owner), release.yml
  found: CI runs `cargo test --workspace` on ubuntu-latest, macos-latest and windows-latest. The real-protocol rows drive `sftp -q -b - -D <local sftp-server>`: Ctx::sh (cfg(unix)) = OpenSSH client -> OpenSSH server on Linux/macOS; Ctx::ps (cfg(windows)) = Win32-OpenSSH sftp.exe -> Win32-OpenSSH sftp-server.exe. No CI leg runs a Windows client against a POSIX host. But the code supports that pairing: release.yml ships x86_64-pc-windows-msvc, remote.rs has Windows-runner handling (render_on(cfg!(windows)), icacls key ACL, NUL device) and nothing refuses an `os: linux` node from a Windows runner. The live gates (lnxvm, wflwvm01) use the macOS client against both host families.
  implication: supported pairings = {OpenSSH client (macOS/Linux)} x {POSIX host, Windows host} and {Win32 sftp.exe} x {Windows host (CI), POSIX host (shipped, untested)}. D2 must parse Win32 client output for both host families.

- timestamp: 2026-10-19T11:07Z
  checked: KL2 measurement on ign-win-836 (az: PowerState/running), as svc-ignition-deploy over the hardened ssh option set. A PowerShell probe (scratchpad kl2/probe.ps1) made a throwaway dir `%USERPROFILE%\ign-kl2-<guid8>\d` (a.json, `b c.txt`, sub), ran `C:\Windows\System32\OpenSSH\sftp.exe -q -b - -D C:\Windows\System32\OpenSSH\sftp-server.exe` (LC_ALL=C.UTF-8, batch `@pwd`, `@ls -la "/C:/.../d"`, `@pwd`, `@ls -lan "/C:/.../d"` LF-joined on stdin) twice: once with stdout as an anonymous PIPE (Process.StandardOutput.BaseStream, the TokioProcessRunner shape) and once redirected to a FILE (Start-Process -RedirectStandardOutput); raw bytes returned base64 and decoded locally (kl2/pipe_out.bin, kl2/file_out.bin). The throwaway dir was removed in a finally (CLEAN=True).
  found: client = OpenSSH_9.5p2 for Windows. Exit 0, stderr empty. 641 bytes, 8 lines, LF=8, CR=8: EVERY line ends in \r\n -- both `Remote working directory:` markers (printf path), the bare server-longname `-la` lines (`-rw-******    1 -        -               2 Oct 19 11:03 a.json\r\n`) and the client-formatted `-lan` lines (`-rw-******    ? 0        0               2 Oct 19 11:03 /C:/Users/svc-ignition-deploy/ign-kl2-d049d061/d/a.json\r\n`). Pipe and file outputs are byte-identical. No `.`/`..` lines from the Win32 server in either shape.
  implication (see next entries for the fix): Win32-OpenSSH sftp.exe writes UNIFORM CRLF on a pipe. D2 stands as written (strip one \r per line only when every line ends in \r\n). The line ending is added in the client's stdout layer for every line it prints (markers, longname lines and lines it formats alike), so a Windows client listing a POSIX host is CRLF-uniform too; there a raw \n inside a POSIX name becomes a line break the same way, and D1's prefix rule still refuses the fragment (it cannot hold `/`), while a name's own trailing \r survives the one-CR strip and is refused. Residual: the Windows-client -> POSIX-host pairing itself was not run (no key for ign-lnx-836 on ign-win-836; not copied on purpose).

- timestamp: 2026-10-19T11:40Z
  checked: D2 safety argument for the supported pairings (rule chosen: D2 as written, `output_lines` in sftp.rs)
  found: the uniform-CRLF decision is fooled only if an LF client prints EVERY line ending in `\r`. A POSIX `ls -lan` listing always holds the client-formatted `<dir>/.` and `<dir>/..` lines (OpenSSH sftp-server returns them), which end in `.`, so with an honest server a POSIX listing is never uniform-CRLF through an LF client and every name's own `\r` survives. A Windows listing holds no line that can end in `\r` (Win32 names and the home cannot hold one). The Win32 client is uniform CRLF (measured), so one `\r` per line is stripped and a POSIX name's own trailing `\r` survives the single strip.
  implication: D2 refuses every ambiguous POSIX listing and still parses every supported pairing; no Windows-only CR relaxation is needed.

- timestamp: 2026-10-19T11:45Z
  checked: GREEN implementation compiled; the six RED tests (`cargo test -p ignition-core --lib -- <six names>`)
  found: 6 passed, 0 failed (963 filtered out). RED bodies untouched.
  implication: D1 + D2 make every RED case refuse (or skip) exactly as specified.

- timestamp: 2026-10-19T11:50Z
  checked: `cargo test -p ignition-core --lib -- deploy::transport` -> 162 passed, 1 failed: process::tests::timeout_kills_child (pid file not found after the 300 ms timeout). Re-ran it 3x: fails every time. Stashed the three fix files (git stash push -- <paths>), ran it at HEAD c1c5781: FAILS identically; popped the stash.
  found: the failure is pre-existing and independent of this fix (process.rs untouched; the test spawns `sh` only). Earlier scratch logs show it passing on this Mac and on ubuntu-latest/windows-latest CI, so it is specific to this agent's sandboxed shell.
  implication: environmental, not a regression; tracked separately in the gauntlet result.

- timestamp: 2026-10-19T12:05Z
  checked: real-protocol suites on this Mac (OpenSSH_10.3p1 client, `sftp -D /usr/libexec/sftp-server`, every POSIX listing now `ls -lan`): `cargo test -p ignition-core --test deploy_transport --test deploy_host_conformance`
  found: deploy_host_conformance 48 passed (local + sh rows), deploy_transport 63 passed.
  implication: the real client's `-lan` output parses under the prefix rule end to end (read, survey, prepare, commit, rollback rows).

- timestamp: 2026-10-19T11:21Z (UTC, gate clock)
  checked: D3 -- `lnxvm_listing_capture` (ported to `LsLongAllNumeric` and now also parsing its own stdout with `parse_batch_listing_as(Posix, Skip, ..)`) against ign-lnx-836 (az: PowerState/running), read-only, `IGNITION_LIVE_LNXVM=1`, env from linuxhost/env.sh, wrapped in `timeout 300 ... </dev/null`; log scratchpad kl2/d3.log (no key/token text in it)
  found: 1 passed. Batch `pwd, ls -lan <home>, pwd, ls -lan <production>` against the Ubuntu 22.04 OpenSSH 8.9 sftp-server (no users-groups extension): every line, `.`/`..` included, is client-formatted with numeric uid/gid and the absolute prefix, e.g. `-rw-rw-r--    ? 0        0             140 Oct 18 16:59 /usr/local/bin/ignition/data/config/resources/production/config-mode.json`. Parsed: home [.bash_logout, .bashrc, .cache, .profile, .ssh], production [config-mode.json] -- the same entries the bare 8.9 capture held. The verbatim capture is now the regression const OPENSSH_8_9_LAN (test posix_listing_parses_captured_ls_lan_output_from_openssh_8_9).
  implication: D1 holds on the real G1 target host; the bare 8.9 capture is a must-refuse input and the -lan capture of the same directories reads identically.

- timestamp: 2026-10-19T12:30Z
  checked: fix-acceptance signal 2 (mutation). cargo-mutants is not installed; 7 hand-seeded mutants at the fix sites (scratchpad kl2/mutants.py, one at a time, source restored and cmp-verified after), each run against `deploy::transport::sftp::tests` + `deploy::transport::remote::tests` (log kl2/mutants2.log)
  found: 7/7 KILLED by test failures (no compile errors). M1 `lines()` split -> killed by the CR RED tests (posix CR, posix/windows strip-only-when-uniform). M2 always strip one CR -> same three. M3 never strip -> both strip-only-when-uniform tests. M4 POSIX bare fallback restored -> the newline RED tests, names-next-to-newline, 8.9 bare must-refuse, either-shape port, parse_ls_listing_cases, spaced-owner port, mixed-CRLF split row. M5 POSIX listed with `ls -la` -> a_posix_host_is_listed_with_ls_lan..., read/prepare_over_a_bare_name_posix_server, batches_never_suppress_errors, symlinks test. M6 account_home skips non-marker lines -> account_home_answers_are_validated. M7 LsLongAllNumeric renders `-la` -> sftp_ops_render_exact_lines + the M5 set.
  implication: mutation score 7/7 at the fix sites; the RED tests assert the root cause, not the symptom.

- timestamp: 2026-10-19T12:35Z
  checked: fix-acceptance signal 5 (revert and reconfirm): `git stash push -- sftp.rs remote.rs live_transport_lnxvm.rs` (tree back to c1c5781), ran the six RED tests; `git stash pop`, ran them again. Also diffed the RED block (`---- a raw newline or carriage return inside a listed name` .. `fn posix_root_cases`) of c1c5781 against the working tree.
  found: reverted -> 0 passed, 6 failed, panics at sftp.rs 2716, 2756, 2796, 2862, 2897, 2922 (the session manager's RED lines); reapplied -> 6 passed. RED block byte-identical (309 lines) to c1c5781.
  implication: this change is what turns RED to GREEN, and no RED test was weakened.

- timestamp: 2026-10-19T12:10Z .. 13:10Z
  checked: gauntlet run 1 (`cargo test --workspace`, then `cargo build -p ignition-cli --no-default-features`, `cargo fmt --all --check`), plus `cargo clippy --workspace --all-targets -- -D warnings` before it (logs kl2/ws-test.log, nodef.log, fmt.log, clippy.log)
  found: clippy exit 0; fmt exit 0; no-default-features build exit 0. cargo test --workspace exit 101: ignition_core lib 970 passed 0 failed (timeout_kills_child passed this time, so it is flaky in this shell, not deterministic), every earlier binary green, then tests/live_gateway.rs `guard_drop_during_unwind_inside_runtime_still_cleans_up` FAILED (the mock never saw the guard's cleanup find + delete), which stopped the run (no --no-fail-fast). Re-ran it twice: fails both times. Stashed the fix (sftp.rs, remote.rs, live_transport_lnxvm.rs, README.md): fails identically at c1c5781. It passed in 9 earlier logged runs (local and CI). It is a wiremock test of the EAM scratch-task guard and touches no deploy transport code.
  implication: pre-existing failure specific to this agent's shell, unrelated to the fix. A --no-fail-fast workspace run covers the binaries after it.

- timestamp: 2026-10-19T13:40Z
  checked: gauntlet run 2 (session manager, resumed after the previous process exited), on the uncommitted fix tree = c1c5781 + sftp.rs, remote.rs, live_transport_lnxvm.rs, README.md. `cargo test --workspace --no-fail-fast`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo build -p ignition-cli --no-default-features` (logs scratchpad kl2/ws2-test.log, clippy2.log, fmt2.log, nodef2.log; status kl2/gauntlet2.status)
  found: test exit 0: 88 test binaries, 2514 passed, 0 failed, 69 ignored. ignition-core lib 970/0, deploy_host_conformance 48/0, deploy_transport 63/0, live_transport_lnxvm 6 passed / 5 ignored (live gates), live_gateway 1 passed / 13 ignored. Both run-1 failures PASSED this run: process::tests::timeout_kills_child and live_gateway guard_drop_during_unwind_inside_runtime_still_cleans_up. Each had failed identically at c1c5781 in run 1 (Evidence 11:50Z, 12:10Z), so they are environment-flaky in the agent shell, not regressions; nothing failed in run 2 to re-check at c1c5781. clippy exit 0, fmt exit 0 (empty output), --no-default-features build exit 0. Placeholder scan of the sftp.rs diff (TODO/FIXME/todo!/unimplemented!/#[ignore]/.only): none.
  implication: the fix is green across the whole workspace; the full gauntlet from Constraints passes.

## Eliminated

- hypothesis: F2 -- a symlink line's `-> target` holding the listed-directory prefix makes `rfind(prefix)` take the target's component as the entry name
  evidence: OpenSSH clients never print link targets in `ls -la` (captured, both shapes); a synthetic line with a prefixed target is refused by the date check (the last two columns before the hit are `<link> ->`).
  timestamp: 2026-10-19T04:32Z

- hypothesis: F1 marker variant -- a name holding `\nRemote working directory: ...` shifts later lines to the next directory
  evidence: the injected marker always adds one marker beyond dirs.len(); captured single- and two-directory batches are REFUSED ("more directory markers than directories").
  timestamp: 2026-10-19T04:29Z

## Resolution

root_cause: (A) Every OpenSSH sftp client prints \n, \r and \t inside a file name raw (utf8.c vasnmprintf), so a POSIX name holding \n reaches the parser as two physical lines; since c67e8bc, `line_columns_and_name` reads any line without the listed-directory prefix as a bare server longname (`split_columns(line, 8)`), so the fragment after the \n is accepted as a forged entry and the real entry is hidden, in the bare, client-formatted and -lan shapes alike. (B) `text.lines()` in `parse_batch_listing_as` strips a \r before \n, so a name ending in \r is read without it and passes safe_remote_path (pre-existing, phase 22).
fix: D1 -- a POSIX host is listed with the new `SftpOp::LsLongAllNumeric` (`@ls -lan "<dir>"`, remote.rs), so every OpenSSH client formats each line with the absolute `<dir>/` prefix; `line_columns_and_name(format, ..)` now refuses a ListFormat::Posix line without the exact prefix (no bare fallback; Windows keeps `@ls -la` and the bare read). D2 -- new `output_lines` splits stdout on `\n` alone (`split_terminator`, so a final `\n` adds no empty line) and strips one trailing `\r` per line only when every line ends in `\r` (Win32-OpenSSH CRLF, measured uniform, KL2); used by `parse_batch_listing_as` and `account_home`. account_home also requires its `@pwd`-only output to be exactly one marker line (a home split by a raw `\n` is refused, not read as its first fragment). Ported: 8.9 bare capture -> must-refuse (both NameRules), either-shape test -> POSIX refuses bare / Windows reads either, spaced-owner test -> Windows bare + prefixed POSIX date forms, windows-only-rules test -> prefixed POSIX rows, `dots` -> prefixed + bare dots refused, FakeSftp answers `@ls -lan` with client-formatted numeric lines whatever `bare_posix`, every `@ls -la "` matcher, live gate lnxvm_listing_capture (-lan + real parse), ListFormat / parse docs, README ssh section. New: OPENSSH_8_9_LAN verbatim D3 capture test, account_home raw-`\n` rows, render rows for LsLongAllNumeric.
oracle_type: specified (the contract in Symptoms.expected: a listing matches the directory exactly or is refused; under NameRule::Skip an unsafe whole name is left out). Boundary neighbours: `x\n` middle/last, `\nfake`, `\n<forged>`, `x\r\n<forged>`, CRLF-uniform `y\r`, mixed line ends, CRLF client with a raw \n.
verification:
  target_test: { result: pass, tests: [posix_listing_refuses_a_name_split_by_a_raw_newline, posix_listing_refuses_a_name_ending_in_a_carriage_return, posix_listing_refuses_names_next_to_a_raw_newline, posix_listing_strips_a_carriage_return_only_when_every_line_ends_in_crlf, windows_listing_strips_a_carriage_return_only_when_every_line_ends_in_crlf, a_posix_host_is_listed_with_ls_lan_and_a_windows_host_with_ls_la] }
  red_before_fix: "c1c5781: 0 passed, 6 failed, all assertion panics (sftp.rs 2716, 2756, 2796, 2862, 2897, 2922), confirmed independently by the session manager; each failure shows the forged `fake`, the misread `y` or the `@ls -la` batch line"
  mutation_check: { result: pass, tool: "manual (cargo-mutants not installed)", score: "7/7 killed", mutants: ["M1 lines() split", "M2 always strip one CR", "M3 never strip", "M4 POSIX bare fallback restored", "M5 POSIX listed with ls -la", "M6 account_home skips non-marker lines", "M7 LsLongAllNumeric renders -la"] }
  revert_and_reconfirm: { result: pass, bug_returned_on_revert: true, fixed_on_reapply: true, note: "fix stashed -> the six RED tests fail at the same lines; reapplied -> 6 passed; RED block byte-identical (309 lines) to c1c5781" }
  no_op_deletion: { result: pass, deletion_justified_by_rca: true, note: "the POSIX bare fallback is removed by D1 (a bare line is indistinguishable from a forged fragment); tests that asserted the bare POSIX read are ported to must-refuse rows, not deleted; the bare 8.9 capture is kept as a must-refuse input and its -lan capture of the same directories is the new positive regression const OPENSSH_8_9_LAN" }
  real_protocol: { result: pass, suites: ["deploy_host_conformance 48/0 (OpenSSH_10.3p1 client, local sftp-server, every POSIX listing ls -lan)", "deploy_transport 63/0"] }
  live_gate_D3: { result: pass, test: lnxvm_listing_capture, host: "ign-lnx-836, Ubuntu 22.04, OpenSSH 8.9 sftp-server", note: "read-only; every line client-formatted with the absolute prefix; parsed home [.bash_logout, .bashrc, .cache, .profile, .ssh], production [config-mode.json]" }
  kl2_measurement: { result: pass, host: ign-win-836, client: "OpenSSH_9.5p2 for Windows", note: "uniform CRLF on a pipe and on a file, markers, bare -la lines and client-formatted -lan lines alike; D2 stands" }
  adjacent_tests: { result: pass, suites_run: ["cargo test --workspace --no-fail-fast (macOS): exit 0, 88 binaries, 2514 passed, 0 failed, 69 ignored", "cargo clippy --workspace --all-targets -- -D warnings: exit 0", "cargo fmt --all --check: exit 0", "cargo build -p ignition-cli --no-default-features: exit 0"] }
  flaky_noted: "run 1 (agent shell) failed process::tests::timeout_kills_child and live_gateway guard_drop_during_unwind_inside_runtime_still_cleans_up, identically at c1c5781 without the fix; both passed in run 2. Pre-existing and environmental, not caused by this fix."
  windows_runtime: { result: not-run-locally, reason: "the windows-latest runner's own sftp.exe is exercised by CI on the next approved push of phase-23-ci (KL2 residual); the Windows client -> POSIX host pairing was argued, not run" }
  human_verify: "not requested: AskUserQuestion was not available to the session manager; the coordinator directed commit and resolve on the gauntlet evidence. Not pushed."
  guardrail_verdict: accepted
fix_commits: [c1c5781 (RED tests), 206998e (fix), 2219faf (live gate harness), ca9cd8f (README)]
files_changed: [crates/ignition-core/src/deploy/transport/sftp.rs, crates/ignition-core/src/deploy/transport/remote.rs, README.md, crates/ignition-core/tests/live_transport_lnxvm.rs (live gate harness, separate commit)]

## Prevention

### Branching 5-Whys (blameless)

- code (A, raw `\n`, regression from c67e8bc): a name holding `\n` was read as two entries.
  - Why? `line_columns_and_name` read any line without the `<dir>/` prefix as a bare server longname.
  - Why was that possible? G1 (23-22) needed Ubuntu 8.9's bare `ls -la` lines to parse, and the plan reasoned about which line shape the server prints, not about which bytes a name can hold. Phase 22's prefix requirement was what anchored each line to a real entry, and dropping it removed the anchor without the plan saying so.
  - Why not seen? Every listing test fed well-formed captures or hostile names that stay on one line (refused by `safe_remote_path`). None fed a name that changes the line structure. That the OpenSSH client prints `\n`, `\r` and `\t` raw (`utf8.c vasnmprintf`) was not checked.
- code (B, trailing `\r`, from phase 22): `str::lines()` drops a `\r` before each `\n`, so a name's own trailing `\r` vanished before the name check.
  - Why? `lines()` is the idiomatic split, and its CR rule is a parser differential when the text embeds untrusted names.
  - Why not seen? The control-character tests used a `\r` in the middle of a name (`cr\rmid`, refused), never at its end.
- AND-gate: yes. The client side (raw `\n`/`\r` in mprintf) is not ours to change. Fixing the parser side of each cause (anchor every POSIX line with `-lan` and the prefix, split on `\n` alone) removes both classes.

### Why not caught

No gate existed for this class: nothing tested the listing parser against a name that alters the line structure. Code review and verification of c67e8bc accepted the bare fallback on its benign captures. The defect was caught after the push to phase-23-ci by a background security review, before it reached main's remote.

### Recurrence guard

- Regression tests in crates/ignition-core/src/deploy/transport/sftp.rs, section "a raw newline or carriage return inside a listed name":
  - the six RED tests above;
  - `posix_listing_refuses_captured_bare_openssh_8_9_output` (must refuse);
  - `posix_listing_parses_captured_ls_lan_output_from_openssh_8_9` (the live D3 capture);
  - `posix_listing_refuses_a_bare_line_and_windows_takes_either_shape`;
  - the account_home raw-`\n` rows.
- In remote.rs, the render rows pin `@ls -lan` for `SftpOp::LsLongAllNumeric`.
- Live gate: `lnxvm_listing_capture` now lists with `-lan` and runs the real parser over its own capture.
- Mutation: 7/7 hand-seeded mutants at the fix sites are killed.
- Pattern: when parsing line-oriented tool output that embeds attacker-influenced names:
  - anchor every line to something a name cannot contain (here the `<dir>/` prefix, since a name holds no `/`);
  - never split it with `str::lines()`;
  - test names holding `\n`, a trailing `\r`, and `\n` followed by a well-formed forged line.
