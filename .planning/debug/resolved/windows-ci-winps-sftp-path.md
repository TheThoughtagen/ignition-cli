---
status: resolved
trigger: "Phase 22 G-22-1 Windows CI (run 38829453966, head 04c3fc3) fails two ignition-core lib tests on windows-latest: winps_enumerate_reports_files_dirs_and_reparse_points and sftp_ops_refuse_hostile_remote_and_local_paths"
created: 2026-10-16T00:45:00Z
updated: 2026-10-16T02:22:00Z
resolved: 2026-10-16T02:22:00Z
goal: find_and_fix
---

## Current Focus

round: final (CI run 38838954722, head ab8608a: all four jobs green, including check (windows-latest))
bug_class: Bohrbug (deterministic test-expectation mismatch on windows-latest)
hypothesis: confirmed. F1 was a latent product defect in ENUMERATE; F2/F3/F4 were unix-premise test oracles against documented Windows-runner behaviour.
next_action: none. Session archived to .planning/debug/resolved/. Fix commits pushed by the orchestrator after user approval (e254cbe..ab8608a, no tags).
fix_commits_round2: [ab8608a (F3+F4, tests/deploy_transport.rs)]
fix_commits_round1: [770b38f (F1, winps.rs), e254cbe (F2, remote.rs)]

reasoning_checkpoint_F1:
  hypothesis: "ENUMERATE yields shifted rel paths because it cuts `$b.Length+1` chars (caller's spelling) off `$f.FullName` (.NET's normalized spelling); on windows-latest .NET expands %TEMP%'s RUNNER~1 to runneradmin (+3), so 'a b' becomes 'ot\\a b'."
  confirming_evidence:
    - "CI diff is exactly a +3 shift; 11-8=3 for runneradmin vs RUNNER~1"
    - "Local pwsh probe: roots `<d>/./root`, `<d>//root`, `<d>/root/` give child FullName `<d>/root/top.json` and Substring results `p.json`/`op.json` (shift -2/-1/-1 relative to caller length)"
    - "New test winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling FAILS on macOS with the current script: ('b', ...), ('p.json', ...) for `./root`"
  falsification_test: "If rel paths built from entry names during the walk still differ from the expected set for any root spelling, the hypothesis is wrong (something other than the prefix cut corrupts rel)."
  fix_rationale: "Building rel = parentRel + $f.Name removes any dependence on how .NET spells the root (8.3 expansion, `.`, `//`, trailing separator, case); it fixes the cause (string-length coupling), not the symptom. full_len stays FullName.Length (unchanged semantics)."
  blind_spots: "Cannot run Windows PowerShell 5.1 locally; relies on PowerShell 5.1 accepting `$s.Push(@($obj,''))`/`$q=$s.Pop()` the same as pwsh 7 (standard .NET method-call semantics). Remote-host wire format keeps `\\` separators."
  candidate_causes:
    - "code: Substring by caller-spelling length in ENUMERATE (confirmed)"
    - "environment: Windows runner TEMP is an 8.3 short path (RUNNER~1) - trigger, not defect"
    - "config: test passes prefix \"\" (no \\\\?\\), so .NET normalizes - production uses \\\\?\\ which skips normalization (why prod never showed it)"
  and_gate: "yes - visible failure needs BOTH the length-coupled Substring (code) AND a root spelling .NET rewrites (environment: 8.3 TEMP + no \\\\?\\ prefix). Fixing the code side removes the whole class; the environment side is not ours to change."

reasoning_checkpoint_F2:
  hypothesis: "sftp_ops_refuse_hostile_remote_and_local_paths fails on Windows because SftpOp::render deliberately normalizes `\\` to `/` in LOCAL paths on a Windows runner (documented), so PathBuf `a\\b` renders as `\"a/b\"`; the test asserts refusal on every platform."
  confirming_evidence:
    - "remote.rs:508-512 `if cfg!(windows) { text.replace('\\\\', \"/\") }` before safe_local_path"
    - "safe_local_path doc: 'On a Windows runner backslashes are normalised to `/` by the caller before this check'; 22-05-SUMMARY.md:204 and 22-13-SUMMARY.md:133 record the normalization as design, never run"
    - "Remote-path assertions in the same test pass on Windows (CI failed only at the local_ops loop, line 1410)"
  falsification_test: "If the Windows render of local `a\\b` produced a line containing a backslash, or the refusal was intended by T-22-18 for local paths, the test premise would be right and product code wrong."
  fix_rationale: "The product behaviour is the documented design and keeps the T-22-18 invariant (no backslash ever reaches a batch line). Fix = make the platform rule explicit and testable on every host: extract the local-path rendering into a function taking `windows_runner: bool`, pin both branches, and assert in the hostile-paths test that on a Windows runner `a\\b` renders as `\"a/b\"` with no backslash."
  blind_spots: "Windows sftp.exe accepting `/` local paths is assumed from the existing design (22-13 live work used Windows host, mac runner); no Windows-runner live run exists."
  candidate_causes:
    - "code (test): premise written before the Windows branch ever ran (confirmed)"
    - "code (product): normalization weakens the gate - eliminated: `\\\\` cannot be part of a Windows file name, so it is always a separator; refusing it would refuse every Windows-runner temp path (sftp.rs:1170 tmp.join)"
  and_gate: "no - single cause: test premise vs documented platform semantics"

## Symptoms

expected: All `check (windows-latest)` steps pass, like ubuntu-latest and macos-latest on the same head (04c3fc3, both green).
actual: `cargo test --workspace` on windows-latest: ignition-core lib test binary 764 passed, 2 failed; cargo stopped, so later Windows test binaries have never run.
errors: |
  F1 winps.rs:815 deploy::transport::winps::tests::winps_enumerate_reports_files_dirs_and_reparse_points
    left:  [("ot/a b", true, 0, false), ("ot/a b/c", ...), ("ot/a b/c/deep.json", ...), ("ot/a b/café.json", false, 3, false), ("ot/top.json", false, 5, false)]
    right: [("a b", ...), ("a b/c", ...), ("a b/c/deep.json", ...), ("a b/café.json", ...), ("top.json", ...)]
  F2 remote.rs:1410 deploy::transport::remote::tests::sftp_ops_refuse_hostile_remote_and_local_paths
    Get { remote: "/r", local: "a\\b" } must be refused
timeline: These tests have never passed on Windows CI. Phase 22 Windows code never ran on CI before: earlier runs died at clippy (fixed b06047e, d267b55), then ubuntu inode test (c888680), then CRLF README test (04c3fc3).
reproduction: GitHub Actions CI workflow, job `check (windows-latest)`, `cargo test --workspace`. Log: `gh run view 38829453966 --log-failed`.

## Constraints

- No local Windows runner. Local tools: cargo-xwin + x86_64-pc-windows-msvc target (compile/clippy only: `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` with PATH=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH and CARGO_TARGET_DIR=$HOME/Library/Caches/cargo-target-ign-cli-xwin). pwsh may exist on macOS for the winps script-logic tests (they run under pwsh on unix too).
- Pushing to origin is outward-facing and needs explicit user approval EACH time (AskUserQuestion). Push only `git push origin main`, never tags (a v* tag starts release.yml).
- No git worktrees (shared CARGO_TARGET_DIR corrupts builds; see .claude/CLAUDE.md).
- F2 has a semantics decision: on Windows `\` is a path separator. Decide whether local paths containing `\` should be refused everywhere (test premise) or accepted as a separator on Windows; check the threat model in .planning/phases/22-transport-plan-pull-verify/22-SECURITY.md and plan threat IDs before choosing. Surface the choice to the user if not determined by existing docs.
- Do not stage `.planning/config.json` or `.planning/research/.cache/`.
- Every fix needs a test that fails before and passes after, provable locally where possible (e.g. pwsh with an 8.3-style or symlinked/differently-cased root for F1).

## Evidence

- timestamp: 2026-10-16T01:00:00Z
  checked: Phase 0 knowledge base (.planning/debug/knowledge-base.md)
  found: No knowledge base file exists; MemPalace not consulted. No known-pattern candidate.
  implication: Investigate from scratch.

- timestamp: 2026-10-16T01:02:00Z
  checked: winps.rs ENUMERATE const (line 78) and the failing assertion arithmetic
  found: `$l=$b.Length+1` (caller spelling of the root) then `$n.Substring($l)` on `$f.FullName` (.NET-resolved spelling). CI output "ot/a b" means the FullName prefix before "root" is exactly 3 chars longer than the caller's ($l lands 2 chars into "root" instead of 1 past "root\"). len("runneradmin") - len("RUNNER~1") = 11 - 8 = 3. GitHub windows runners set TEMP=C:\Users\RUNNER~1\AppData\Local\Temp; tempfile uses it; the test passes prefix "" (no \\?\), so .NET normalizes the path and expands the 8.3 component.
  implication: F1 is a script-logic defect (rel derived from caller spelling length, not from the walk); any root spelling that .NET rewrites (8.3, `.`, `..`, `//`, trailing separator) shifts every rel path.

- timestamp: 2026-10-16T01:04:00Z
  checked: winps.rs test powershell() lookup on Windows
  found: looks for files named "pwsh", "powershell", "powershell.exe" on PATH; on Windows "pwsh" has a .exe so only "powershell.exe" (Windows PowerShell 5.1, .NET Framework) matches.
  implication: CI ran ENUMERATE under .NET Framework, which also expands short names during normalization; same family the remote host runs (`powershell`).

- timestamp: 2026-10-16T01:06:00Z
  checked: sftp.rs cross_check (line 820-900) production caller
  found: roots are `windows_native_path(abs(chain))` from the sftp-resolved absolute path, sent with LONG_PATH_PREFIX `\\?\`; .NET does not normalize `\\?\` paths, so production FullName prefix normally equals the caller spelling. Live captures (22-13) exercised the walk.
  implication: Production impact of F1 is latent (fragile, not observed live). Fix still belongs in the script: derive rel from names, not from string length of the caller's spelling.

- timestamp: 2026-10-16T01:08:00Z
  checked: remote.rs SftpOp::render local closure (lines 504-515), safe_local_path doc (93-98), safe_remote_path table (64), 22-05-SUMMARY.md:204, 22-13-SUMMARY.md:133, 22-SECURITY.md T-22-18
  found: On cfg!(windows) the local path text has `\` replaced by `/` BEFORE safe_local_path; doc comments state "On a Windows runner backslashes are normalised to `/` by the caller before this check"; 22-05-SUMMARY records "Windows-runner behaviour of safe_local_path (backslash normalisation) is compiled but not run here"; T-22-18 mitigation is `safe_remote_path` refusing backslash (remote side, unchanged). Production local paths are `tmp.join(..)` (sftp.rs:1170) i.e. `C:\Users\...\Temp\...` on a Windows runner: refusing `\` everywhere would refuse every Windows-runner download/upload.
  implication: F2 semantics are settled by existing docs + design: on a Windows runner `\` in a LOCAL path is a separator and is normalized; the batch line never carries a backslash (T-22-18 invariant holds). The failing assertion is the test premise, written before the Windows branch ever ran. No user decision needed.

- timestamp: 2026-10-16T01:10:00Z
  checked: CI log of run 38829453966 (gh run view --log-failed)
  found: Only the two named tests fail in ignition-core lib (764 passed, 2 failed). Full messages match the symptoms.
  implication: Scope is exactly F1 + F2 for this binary.

- timestamp: 2026-10-16T02:30:00Z
  checked: CI run 38836275411 (head e254cbe), log scratchpad/win4.log lines ~1740-1958
  found: ignition-core lib 768 passed 0 failed (F1, F2 fixed). Next binary ignition-core --test deploy_transport 28 passed 2 failed. F3 ssh_g_reports_pinned_options: Windows ssh -G reports `globalknownhostsfile NUL` and `userknownhostsfile C:/Users/RUNNER~1/AppData/Local/Temp/ign-ssh-8x7x7N/known_hosts`; test expects `/dev/null`. F4 ssh_g_keeps_odd_paths_literal: expected `userknownhostsfile C:\Users\RUNNER~1\...\ign odd %h RpQF5u\known hosts`, got the same with `/` separators (spaces and literal %h preserved). ssh_g_reports_identity_and_configured_timeouts PASSED on Windows.
  implication: Cargo stopped at deploy_transport; every later Windows test binary is still unobserved.

- timestamp: 2026-10-16T02:34:00Z
  checked: remote.rs NULL_DEVICE (cfg(windows) "NUL" / else "/dev/null"), option_path (`if cfg!(windows) { text.replace('\\', "/") }`, doc "forward slashes on a Windows runner"), ssh_argv/sftp_argv (`-i` gets identity.as_os_str() unconverted)
  found: Both reported values are what the product sends. Windows ssh -G does NOT rewrite backslashes itself: the identity path goes through `-i` raw and the identity test asserting `identityfile {display()}` (backslashes) passed on Windows. The `/` spelling comes from option_path (OpenSSH config-value parsing treats `\` as an escape inside quoted values, so the product converts on purpose).
  implication: F3/F4 are unix-premise test expectations; product is correct by design. Fix the test oracle to the documented platform rule (NULL_DEVICE constant; `\`->`/` on a Windows runner for option paths).

- timestamp: 2026-10-16T03:20:00Z
  checked: F3/F4 fix RED/GREEN on macOS (deploy_transport.rs)
  found: New test reported_spellings_match_the_windows_runner_observation pins the exact CI-observed Windows lines (`NUL`; `C:/Users/RUNNER~1/.../ign-ssh-8x7x7N/known_hosts`; `C:/Users/RUNNER~1/.../ign odd %h RpQF5u/known hosts`). With the helpers written as the old premise (verbatim text, `/dev/null`) it FAILED (`left: "/dev/null" right: "NUL"`); with the Windows-runner rule it passes. The two ssh -G tests now take their expected null device and option path from the helpers. deploy_transport on macOS: 62 passed.
  implication: F3/F4 oracle fixed; mutants (reported_path_text -> identity; reported_null_device(cfg!(windows)) -> (true)) both killed.

- timestamp: 2026-10-16T03:25:00Z
  checked: Proactive sweep of every Windows test binary not yet run on CI (run order from win4.log: all ignition-cli binaries and ignition-core lib + api_classify..deploy_render already PASSED on Windows; deploy_transport ran fully, 28+2). Unrun - ignition-core: deploy_verify, e2e_contract, eam_contract, edit_pipeline, gateway_info_contract, keyring_smoke, lint_contract, live_* (x6), logs_contract, module_fetch_contract, module_uninstall_contract, projects_contract, resources_contract, restart_wait_contract, rig_module_override, script_contract, scripts_codec_contract, session_contract, session_login_contract, sessions_contract, status_contract, tags_contract, testing_contract, trial_contract, webdev_contract, workspace_checkout, workspace_path_mapping, workspace_status_push; ignition-tui lib tests; doctests.
  found: |
    No test proven to fail on Windows, so no F5+ fix was made. Patterns checked and why each holds:
    - sh-stub / $EDITOR / chmod harnesses (e2e_contract, edit_pipeline, lint_contract, module_fetch 0o600, rig_module_override 0o755): every such test and helper is #[cfg(unix)]; the ungated ones (empty-PATH refusals, resource-path refusals) spawn nothing.
    - live_*: all network tests #[ignore]; the ungated ones (target YAML load, scratch-dir shape, oq5 corpus/uuid/percentile, local_url_guard, guard_drop_during_unwind) are pure or wiremock. walk_corpus already maps `\` to `/`; the corpus is under fixtures/deploy/** (-text, byte-identical checkout).
    - Path text in messages (rig_module_override contains(display())): product formats the same Path with display(); override_path uses std::path::absolute (GetFullPathNameW, no 8.3 expansion), so the tempdir spelling survives. relative_artifact_path keeps `relative/...` verbatim in display().
    - `/`-joined rel paths (workspace untracked walk, codec manifest keys, tree_relative_string): product joins components with `/` on both sides; the tree strictness walk compares PathBuf by components (separator-agnostic).
    - CRLF checkout: include_str!/README/source checks in unrun tests use lines()/split_whitespace()/contains (CRLF-safe); webdev route assertions use contains; fixture XML/deploy dirs are -text.
    - Path-shaped TUI fixtures (`/rigs/docker/compose.yml`) only go through display()/fake runners; TUI config tests isolate via IGNITION_CLI_CONFIG + IGNITION_TOKEN_<P> env (no dirs/keyring dependence).
    - Timing: floors use tokio sleep (never early on Windows); upper bounds are seconds-scale.
    - Doctests: every doc fence is json/jsonc/text or bare fence closings - none compile or run.
    - Final CI step `cargo build -p ignition-cli --no-default-features`: `cargo xwin check --target x86_64-pc-windows-msvc -p ignition-cli --no-default-features` passes.
  implication: Residual risk is runtime behaviour this static review cannot see (Windows file locking on rename/remove, external tools). The next CI round is still required to observe the rest of the Windows binaries.

- timestamp: 2026-10-16T02:22:00Z
  checked: Human-verify checkpoint response + CI run 38838954722 (head ab8608a3d64229eb12cd25bcd5092cd76c23b94b), `gh run view --json` and the full `check (windows-latest)` job log (job 116774350095)
  found: |
    User confirmed fixed and approved the push; orchestrator ran `git push origin main` (e254cbe..ab8608a, no tags).
    Run conclusion success. Jobs: check (ubuntu-latest) success, check (macos-latest) success, check (windows-latest) success, keyring-smoke success.
    Windows job log: 84 `test result: ok` lines, zero FAILED/panicked. ignition-core lib 768 passed 0 failed; deploy_transport 31 passed 0 failed; ignition-tui lib 281 passed 0 failed; the final `--no-default-features` build of ignition-cli finished.
    All seven target tests report `ok` on Windows: winps_enumerate_reports_files_dirs_and_reparse_points, winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling, sftp_ops_refuse_hostile_remote_and_local_paths, local_backslashes_are_separators_only_on_a_windows_runner, reported_spellings_match_the_windows_runner_observation, ssh_g_reports_pinned_options, ssh_g_keeps_odd_paths_literal.
  implication: F1-F4 fixed on the real Windows runner, and every Windows test binary that had never run before (the static sweep's residual risk) also passed. No F5+ finding. Session can close.

## Eliminated

- hypothesis: "F2: the product's Windows-runner normalisation of `\\` to `/` in LOCAL paths weakens the T-22-18 gate, so the test premise (refuse `\\` everywhere) is right"
  evidence: "`\\` cannot be part of a Windows file name, so on a Windows runner it is always a separator; every Windows-runner temp path (sftp.rs tmp.join) contains it. T-22-18's mitigation is safe_remote_path refusing `\\` on the REMOTE side, unchanged. The rendered batch line never carries a backslash on either branch."
  timestamp: 2026-10-16T01:08:00Z

- hypothesis: "F3/F4: Windows OpenSSH `ssh -G` itself rewrites backslashes to `/` in reported paths"
  evidence: "The identity path goes through `-i` unconverted and ssh_g_reports_identity_and_configured_timeouts (expects backslashes) passed on Windows; the `/` spelling comes from the product's option_path."
  timestamp: 2026-10-16T02:34:00Z

## Resolution

root_cause: |
  F1 (product, latent): ENUMERATE derived each relative path as FullName.Substring(callerRoot.Length+1). .NET rewrites a root it normalises (8.3 expansion RUNNER~1 -> runneradmin, `.`, doubled or trailing separators), so the caller's spelling and FullName need not share a prefix length. On windows-latest the test root sits under %TEMP%=C:\Users\RUNNER~1\..., sent without \\?\, so every rel lost 3 leading chars. AND-gate: code (length coupling) + environment (8.3 TEMP, no \\?\ in the test). Production sends \\?\ (no normalisation), which is why live runs never showed it.
  F3/F4 (test premise): deploy_transport ssh -G tests expected the unix spelling of two options the product sets differently on a Windows runner by design: GlobalKnownHostsFile = NULL_DEVICE (`NUL` on Windows) and UserKnownHostsFile through option_path, which writes `\` as `/` on a Windows runner. ssh echoes the value (identity via `-i` stays unconverted and its backslash expectation passed on Windows).
  F2 (test premise): SftpOp::render normalises `\` -> `/` in LOCAL paths on a Windows runner by documented design (safe_local_path doc, 22-05-SUMMARY:204, 22-13-SUMMARY:133); `\` cannot be part of a Windows file name and every Windows-runner temp path holds it. sftp_ops_refuse_hostile_remote_and_local_paths asserted refusal of local `a\b` on every platform; it was written before the Windows branch ever ran.
fix: |
  F1: ENUMERATE now pushes (DirectoryInfo, relPrefix) pairs and builds rel = relPrefix + $f.Name (`\`-joined, same wire form as before on a Windows host); full_len stays FullName.Length. Doc comment explains why.
  F3/F4: expected lines come from reported_null_device(windows_runner) and reported_path_text(text, windows_runner) (`\`->`/` on a Windows runner); new test reported_spellings_match_the_windows_runner_observation pins both against the exact CI-observed Windows lines on every host.
  F2: SftpOp::render delegates to private render_on(windows_runner) (render = render_on(cfg!(windows))), so both runner branches are tested on every host. Hostile-paths test checks remote and local ops on both branches; on a Windows runner local `a\b` must render as "a/b" with no backslash. New test pins realistic Windows-runner paths and that normalising a separator never launders another refused character; remote `\` refused on both branches.
verification:
  target_test: { result: pass, tests: [winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling, winps_enumerate_reports_files_dirs_and_reparse_points, sftp_ops_refuse_hostile_remote_and_local_paths, local_backslashes_are_separators_only_on_a_windows_runner] }
  red_before_fix:
    F1: "macOS pwsh 7.5.4: new test FAILED with root `<tmp>/./root` -> [('b',..),('p.json',..)] (same shift class as CI 'ot/a b')"
    F2: "macOS: hostile-paths test on render_on(true) FAILED with `Get { remote: \"/r\", local: \"a\\\\b\" } must be refused (windows_runner true)` - the exact CI message"
  mutation_check: { result: pass, tool: "manual (no Stryker for Rust)", mutants: ["ENUMERATE `$m=$q[1]+$f.Name` -> `$m=$f.Name`: killed by both enumerate tests", "render_on `if windows_runner` -> `if cfg!(windows)`: killed by both F2 tests", "render_on `if windows_runner` -> `if true`: killed by both F2 tests"] }
  no_op_deletion: { result: pass, deletion_justified_by_rca: true, note: "F1 replaces the Substring cut with walk-built rel (no behaviour removed). F2 product change is a pure extraction; the one relaxed test branch (local a\\b on a Windows runner) is replaced by a stronger positive assertion (exact rendering, no backslash) justified by RCA." }
  adjacent_tests: { result: pass, suites_run: ["cargo test --workspace (macOS): exit 0, 84 binaries, 2150 passed, 0 failed, 51 ignored", "cargo clippy --workspace --all-targets -- -D warnings (macOS host): clean", "cargo fmt --all --check: clean"] }
  revert_and_reconfirm: { result: pass, bug_returned_on_revert: true, fixed_on_reapply: true, note: "F1: original ENUMERATE const restored from HEAD -> new test FAILED; fixed const restored -> both pass. F2: original premise on render_on(true) -> FAILED (RED run above); corrected premise -> pass." }
  windows_target_clippy: { result: pass, command: "cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings" }
  windows_runtime: { result: not-run-locally, reason: "no Windows runner/wine; needs windows-latest CI after an approved push" }
  guardrail_verdict: accepted
files_changed:
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/tests/deploy_transport.rs
verification_round2:
  target_test: { result: pass, tests: [reported_spellings_match_the_windows_runner_observation, ssh_g_reports_pinned_options, ssh_g_keeps_odd_paths_literal] }
  red_before_fix: "helpers as the old premise (verbatim text, /dev/null): pinned test FAILED left \"/dev/null\" right \"NUL\""
  mutation_check: { result: pass, mutants: ["reported_path_text Windows branch -> identity: killed (left C:\\Users\\... right C:/Users/...)", "reported_null_device(cfg!(windows)) -> (true) in ssh_g_reports_pinned_options: killed on macOS (did not report `globalknownhostsfile NUL`)"] }
  no_op_deletion: { result: pass, note: "only test oracle changed; the removed literal `/dev/null` line is replaced by a platform-derived assertion of the same option" }
  adjacent_tests: { result: pass, suites_run: ["cargo test --workspace (macOS): exit 0, 84 binaries, 2151 passed, 0 failed, 51 ignored", "cargo clippy --workspace --all-targets -- -D warnings: clean", "cargo fmt --all --check: clean"] }
  windows_target_clippy: { result: pass, command: "cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings" }
  windows_lean_build_check: { result: pass, command: "cargo xwin check --target x86_64-pc-windows-msvc -p ignition-cli --no-default-features" }
  windows_runtime: { result: not-run-locally, reason: "ssh -G on Windows OpenSSH needs windows-latest CI" }
  revert_and_reconfirm: { result: pass, note: "RED with the old-premise helpers, GREEN with the Windows rule; ssh_g_* on macOS unchanged (no backslashes, /dev/null)" }
  guardrail_verdict: accepted
verification_ci:
  windows_runtime: { result: pass, run: 38838954722, url: "https://github.com/TheThoughtagen/ignition-cli/actions/runs/38838954722", head: ab8608a3d64229eb12cd25bcd5092cd76c23b94b }
  jobs: { "check (ubuntu-latest)": success, "check (macos-latest)": success, "check (windows-latest)": success, keyring-smoke: success }
  windows_binaries: "84 `test result: ok`, 0 FAILED; ignition-core lib 768/0, deploy_transport 31/0, ignition-tui lib 281/0; --no-default-features build finished"
  human_verify: "confirmed fixed (user), push approved; orchestrator pushed e254cbe..ab8608a to origin/main, no tags"
fix_commits: [770b38f, e254cbe, ab8608a]
oracle_type: "specified (expected rel-path set from the fixture tree; exact rendered batch line; exact CI-observed ssh -G lines)"

## Prevention

### Branching 5-Whys (blameless)

- code (F1, product): rel paths were wrong on Windows.
  - Why? ENUMERATE cut `callerRoot.Length+1` chars off `FullName`.
  - Why was that possible? The script assumed .NET echoes the caller's spelling of the root. That holds for the production input (`\\?\`-prefixed, never normalised) and for the unix test tempdirs, so the coupling never showed.
  - Why not seen? No test fed a root spelling that .NET rewrites (8.3 short name, `.`, doubled or trailing separator). Fixed by building rel from the walk (`relPrefix + $f.Name`) and by a test that feeds rewritten spellings on every host.
- code (F2/F3/F4, test oracles): three tests encoded the unix spelling (refuse local `\\`, `/dev/null`, backslash display()).
  - Why? They were written while the `cfg!(windows)` branches (SftpOp local render, NULL_DEVICE, option_path) were compiled but never executed. 22-05-SUMMARY:204 records "compiled but not run here".
  - Why was that possible? The platform rule sat behind `cfg!(windows)`, so no macOS or Linux host could run the Windows branch, and its behaviour was documented in comments only, not in a test.
  - Fix: platform branches now take an explicit `windows_runner: bool` (SftpOp::render_on, reported_null_device, reported_path_text), so every host tests both branches against the exact CI-observed Windows lines.
- environment/CI: the `check (windows-latest)` gate existed but never reached Phase 22 tests.
  - Why? Earlier steps failed first on successive pushes (clippy b06047e/d267b55, the ubuntu inode test c888680, the CRLF README test 04c3fc3). `cargo test --workspace` also stops at the first failing test binary, so later binaries stayed unobserved and failures surfaced one binary per round (lib first, then deploy_transport).
  - Why was that possible? The Windows job stayed red across several pushes, so it gave no per-test signal, and locally only compile/clippy coverage exists (cargo-xwin, no Windows runtime).
- AND-gate (F1): yes. Code (length coupling) AND environment (8.3 TEMP on the runner, no `\\?\` in the test). The code fix removes the whole class. The environment side is not ours to change.

### Why not caught

The gate that should have caught this was the `check (windows-latest)` CI job. It existed but never executed the Phase 22 tests because earlier steps on the same job failed first. The macOS and ubuntu test gates cannot see `cfg!(windows)` branches. Local verification for Windows was compile/clippy only (cargo-xwin). Review and verify accepted "compiled but not run" Windows behaviour (22-05-SUMMARY:204) with no host-independent test of the Windows branch. For F1, no test exercised a root spelling .NET normalises, on any host.

### Recurrence guard

- Regression tests, run on every host (pwsh is required and the winps tests never skip; see the winps.rs `powershell()` helper):
  - crates/ignition-core/src/deploy/transport/winps.rs: `winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling` (roots `./root`, `//root`, trailing separator); F1 class.
  - crates/ignition-core/src/deploy/transport/remote.rs: `sftp_ops_refuse_hostile_remote_and_local_paths` and `local_backslashes_are_separators_only_on_a_windows_runner`, both over `render_on(true)` and `render_on(false)`; F2 class.
  - crates/ignition-core/tests/deploy_transport.rs: `reported_spellings_match_the_windows_runner_observation` pins the exact CI-observed Windows `ssh -G` lines; `ssh_g_reports_pinned_options` and `ssh_g_keeps_odd_paths_literal` take expectations from `reported_null_device` / `reported_path_text`; F3/F4 class.
- Design pattern: put Windows-runner behaviour behind an explicit `windows_runner: bool` parameter (with `cfg!(windows)` only at the call site), so both branches run on every host.
- Live gate: `check (windows-latest)` is green end to end (run 38838954722, 84 binaries), so it now produces a per-test signal.
- Verified present at HEAD and `ok` in the Windows job log of run 38838954722 (all seven named tests).
- Optional follow-up (not done here, outside this session's scope): `cargo test --workspace --no-fail-fast` in CI would surface all failing binaries in one round instead of one per push.
