---
phase: quick-261104-ava
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-core/tests/deploy_host_conformance.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - README.md
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - .planning/quick/261104-ava-pr35-round2-win-ci-sftp/261104-ava-SUMMARY.md
autonomous: true
requirements: [QUICK-ava]

estimate:
  tokens: 190000
  raw_tokens: 190000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "W1: case (e) of `core_probe_walks_every_folder_on_a_touched_path` makes its junction through one test helper `make_junction` that passes backslash-only paths (`native_path`) to `cmd /C mklink /J`; `ps_junction_target_dir_is_refused` uses the same helper. Case (e) still asserts the probe refuses and that nothing was written through the junction. A non-Windows unit test pins `native_path`. The Windows CI leg is the real check"
    - "N2: on a Windows sftp host, `list_tree` probes every requested `<module>/<type>` folder the sftp listing did not return, in the same PowerShell enumeration call as the folders it did return. A folder the host finds is `transport_failed` naming it (`did not return`); a folder absent on the host too is still left out, with no error"
    - "N3: `cross_check` charges each long-path entry the sftp listing did not return (files and folders) against the read's `Budget`. Past `READ_MAX_ENTRIES` it is `transport_failed` with `more than {READ_MAX_ENTRIES} entries under the data directory (at <root>)`, the `list_level` wording. A long entry sftp did return is not charged twice. Both callers (`read`, `list_tree`) pass their budget"
    - "N1: README (the `deploy apply` Commands row, the `What is still refused` bullet, the `deploy plan` `Redundant pairs` paragraph and apply step 5) says an `unmanaged` tag unit in a pair's mode collection is not refused, not named by `deploy plan`, and not folder-listing residue; its mode files stay, keep the backup from syncing, and `deploy verify` warns about them. This matches `pair_tag_keys` and the C3 read set, which existing tests pin"
    - "N4: README (the C12 sentence and apply step 10), the `apply/mod.rs` branch table and the `deploy_pair_tags_refused` message and hint say what a late re-list refusal leaves: no collection file written, no lock taken, the run's on-box staging and snapshot dropped, the run directory with its `.gwbk` and `result.json` kept. The C12 test pins the `.gwbk` and `result.json`. Slug and exit code unchanged"
    - "Three commits on `feat/deploy-tags-core` after d5efc63: `test(261104-ava): ... (W1)`, `fix(261104-ava): ... (N2, N3)`, `docs(261104-ava): ... (N1, N4)`. fmt, workspace clippy, `cargo test --workspace` and windows-msvc xwin clippy are green. Goldens are byte-identical to d5efc63. No slug, exit-code, Cargo.toml or Cargo.lock change"
  artifacts:
    - path: "crates/ignition-core/tests/deploy_host_conformance.rs"
      provides: "`native_path`, `make_junction` (cfg windows), and the pin `native_path_uses_backslashes_only`"
      contains: "fn native_path(p: &Path) -> String"
    - path: "crates/ignition-core/src/deploy/transport/sftp.rs"
      provides: "`cross_check(found, absent, budget)`: probes requested folders sftp omitted (N2) and charges unlisted long entries (N3), with the two new Windows tests"
      contains: "absent: &[Vec<String>],"
    - path: "README.md"
      provides: "the unmanaged exception (N1) and what a late re-list refusal leaves (N4)"
      contains: "unmanaged"
    - path: ".planning/quick/261104-ava-pr35-round2-win-ci-sftp/261104-ava-SUMMARY.md"
      provides: "one line per item W1, N1-N4, RED evidence, commits, gate token"
      contains: "## Findings"
  key_links:
    - from: "SftpTransport::list_tree (Windows)"
      to: "cross_check(&found, &absent, &mut budget) -> enumerate_roots"
      via: "one PowerShell enumeration over the found roots and the requested folders sftp did not return"
      pattern: "self.cross_check(&found, &absent, &mut budget)"
    - from: "SftpTransport::read (Windows)"
      to: "cross_check(&found, &[], &mut budget)"
      via: "the same long-entry charge against the read's budget"
      pattern: "self.cross_check(&found, &[], &mut budget)"
    - from: "core_probe_walks_every_folder_on_a_touched_path case (e), ps_junction_target_dir_is_refused"
      to: "make_junction -> native_path -> cmd /C mklink /J"
      via: "backslash-only arguments"
      pattern: "make_junction("
---

<objective>
**Execution: three tasks in order, one commit per task, each commit green on its own.**

| Task | Content | Commit | Gate token |
|------|---------|--------|------------|
| 1 (tracer) | W1: native paths for `mklink /J` in the conformance junction cases | `test(261104-ava): ... (W1)` | `T1-OK` |
| 2 | N2 + N3: the Windows `list_tree` completeness probe and the long-entry budget | `fix(261104-ava): ... (N2, N3)` | `T2-OK` |
| 3 | N1 + N4: README, `apply/mod.rs` and `error.rs` text, one C12 test pin, SUMMARY, full gate | `docs(261104-ava): ... (N1, N4)` | `FULL-GATE-OK` |

This task fixes the one failing check on PR #35 (Windows CI run 41848470336) and CodeRabbit round-2 comments N1-N4. Branch `feat/deploy-tags-core`, base HEAD d5efc63.

**RED rule.** Each code item (W1, N2, N3) starts with a failing offline test where the host allows, recorded for SUMMARY before the fix. W1 cannot run on macOS: its RED is the CI log, plus a unit test of the new path helper that fails to compile before the helper exists. A code item whose test passes before any change does not reproduce: no code change, keep the test as a pin, record `<id>: not reproduced (<reason>)`. N1 and N4 are docs; N4 adds one assertion pin to an existing test.

Source coverage:

| Item | Source | Task |
|------|--------|------|
| W1 Windows CI: `mklink /J` gets forward slashes (`Invalid switch - "resources"`) | CI run 41848470336 | 1 |
| W1 other mklink/junction sites checked | orchestrator | 1 |
| N2 (Major) Windows `list_tree` skips a requested folder sftp omits | CodeRabbit | 2 |
| N3 long entries not counted against `READ_MAX_ENTRIES` | CodeRabbit | 2 |
| N1 README: the `unmanaged` exception to the pair tag refusal | CodeRabbit | 3 |
| N4 README / branch table / hint: what a late re-list refusal leaves | CodeRabbit | 3 |
| Gates, goldens, hygiene, SUMMARY | orchestrator constraints | every task; full gate in 3 |

Purpose: the Windows CI leg goes green on a real junction check, a Windows pair host can no longer hide a mode tag type folder (or an unbounded long-path tree) from the residue check, and the README says exactly what is refused and what a refusal leaves.

Output: test helper, sftp fix with tests, docs, SUMMARY. No version bump, tag, push, PR reply, worktree or production call.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/quick/261104-7cy-coderabbit-review-fixes-pr35/261104-7cy-SUMMARY.md

Line numbers are as of d5efc63. Grep the quoted anchor first, then read around it.

**Planner findings (binding; do not re-investigate):**

- **W1 mechanics.**
  - `Ctx::core_root()` (`deploy_host_conformance.rs` about 202) is `root().join("config/resources/core")`. On Windows that `PathBuf` keeps the forward slashes, and case (e) (about 1794-1815) passes it raw to `cmd /C mklink /J`, which reads `/resources` as a switch. Both `conformance_local_` and `conformance_ps_core_probe_walks_every_folder_on_a_touched_path` panic at the `mklink /J` assert (about 1807). The tempdir target already has backslashes.
  - `ps_junction_target_dir_is_refused` (about 2746-2777) already avoids the bug with a local closure `native` (`p.display().to_string().replace('/', "\\")`) and `.args(["/C", "mklink", "/J"])`. Hoist that into file-level helpers and use them at both sites.
  - Other junction sites, checked: `live_project_host.rs` about 597-605 builds its paths with PowerShell `Join-Path` (native), and `live_transport_wflwvm01.rs` `MAKE_JUNCTION` (about 260) joins backslash-built parameters. Neither has the bug; no change there.
  - `std::fs::remove_dir(&link)` with forward slashes is fine on Win32 and removes the junction, not its target.
  - On the local host, Rust's `symlink_metadata` reports a junction as a symlink, so case (e) is expected to refuse once `mklink` succeeds. If the Windows leg then fails at the refusal assertion, that is a real C6 finding for a follow-up, not something to fix blind here.
- **N2 mechanics.**
  - `SftpTransport::list_tree` (`sftp.rs` about 1767-1840) calls `discover`, then on Windows `cross_check(&found)` (about 1796). `cross_check` (about 949-1052) enumerates only `found.roots`, the requested chains sftp returned.
  - A requested `config/resources/<mode>/ignition/<type>` folder that the sftp server drops from its parent's listing (a host path of 260 characters or more, C7a-8), or that sits below an omitted `ignition` folder, is never a root. `list_tree` then leaves it out (about 1807: `if !found.roots.contains(&chain) { continue; }`), so plan, verify and the C12 re-list see no residue.
  - `enumerate_roots(&roots)` (about 1057) takes any native roots, returns `None` for a root that does not exist, and chunks the call itself. So the missing folders can join the same call: no extra PowerShell session.
  - **Planner decision: fail, not list.** A requested folder the host finds but sftp did not return is `transport_failed`. Reasons: `list_tree` is names-only and has no unreadable list, which is the rule `cross_check_names` (about 1117-1127, WR-03) already applies ("transport_failed naming it, whatever its length"); and either way the deploy cannot proceed, but a transport failure names the cause (the sftp server) instead of residue the operator cannot see over sftp. Record this in SUMMARY.
  - Scope: `read()` passes no missing folders (`&[]`), unchanged behaviour. Its own scoped roots have the same shape (a scoped root sftp omits is read as absent); record that in SUMMARY as an observation, not fixed here (outside N2).
- **N3 mechanics.**
  - `list_level_as` (about 743-797) charges every parsed entry to the read's `Budget` (`budget.entries`, `READ_MAX_ENTRIES` = 200 000) and fails with `more than {READ_MAX_ENTRIES} entries under the data directory (at {})`.
  - `cross_check` pushes every host entry at or over `LONG_PATH_LIMIT` (240, `winps.rs` 71) into `long` uncounted. Those sftp did not return were never parsed by `list_level`, so a long-path tree can exceed the cap unbounded. `list_tree` (about 1824) and `read` (about 1853) both consume `long`.
  - The charge goes in `cross_check`, per root, so both callers are bounded: count the root's long entries that the sftp listing did not return (a file whose collection-relative path is not among `found.files` of that collection; a folder whose chain is not in `found.dirs`), add them to `budget.entries`, and fail past the cap. Long entries sftp did return were already charged by `list_level`.
  - 200 000 entries are too many for the fake. The N3 test calls `discover` and `cross_check` directly (private, same module) with a budget pre-set near the cap.
- **Test doubles (sftp.rs tests module).** `windows_fake()`, `windows_tree(&fake)`, `windows_transport(&fake)` (about 6635), `detail(err)`. Knobs (about 3449-3515): `hidden` (a path left out of its parent's listing; a hidden folder is never descended), `enum_len_for` (a native path length for one enumerated path, for example 262), `enum_scripts` (one entry per PowerShell call). The enumeration fake (about 3700-3790) returns `"x": 0` for a root that is not a directory. The idiom is `windows_list_tree_is_checked_against_a_walk` (about 6797-6860).
- **N1 mechanics (verified in code; pinned already).**
  - `plan::pair_tag_keys` (`plan.rs` about 519-539) filters out `PlanStatus::Unmanaged`. An `unmanaged` tag unit (live in the mode collection, neither declared nor in the ledger) is never refused, and the `deploy plan` pair warning (`actions/deploy.rs` `pair_tags_warning`, about 514-536) never names it.
  - `plan::pair_mode_tag_dirs` (about 578): the read set is every bare row with a live sha "whatever their status", so its files are not folder-listing residue either.
  - `deploy verify` warns: `pair_mode_tag_files` includes `unmanaged` (about 546), and the `others` line in `deploy_verify_planned` (`actions/deploy.rs` about 1323) names undeclared units' mode files.
  - Pins: `pair_tags_written_or_kept_in_the_mode_collection_are_refused_on_a_pair_only` (`apply/gates.rs`, the `.../Unmanaged` entry is not among the refused keys), `pair_mode_tag_dirs_names_only_folders_with_no_read_file` (`tests/deploy_plan.rs` about 3459, an `Unmanaged` entry's read file is not residue), `pair_verify_warns_on_mode_tag_files` (`tests/deploy_pair.rs` about 508).
  - The README does not say this. The `deploy apply` Commands row (line 585), `- **What is still refused.**` (about 2684-2702), the `deploy plan` paragraph `**Redundant pairs.** For a target with` (about 3096-3105) and apply step `5. For a target with` (about 3724-3732) all say any unledgered mode tag file is refused or warned.
- **N4 mechanics.**
  - `apply::drive` (`apply/mod.rs` about 1307-1330): the C12 refusal calls `ctx.host.discard_interrupted(run_id)`, which drops `journal.json`, `snapshots/<run>` and `staging/<run>` on the host (`apply/host.rs` about 438-441), and returns the error. `run_apply`'s one exit point then writes `result.json`. By then the run directory exists, holding `run.json`, `plan.json`, `events.ndjson`, `read_back_baseline.json` and the downloaded `<run id>.gwbk` (unless `--skip-gwbk`; README "The run directory", about 3944-3950). Confirm the list against `run_apply` / `drive` before writing it.
  - Wrong today: the README C12 sentence (about 2698-2700) ends by saying nothing was written. The `apply/mod.rs` branch table's lock-refused row (line 76) says "nothing was written", though the same run directory and `.gwbk` exist. The `deploy_pair_tags_refused` message (`error.rs` `deploy_pair_tags_refused_text`, about 1129-1137) and hint (about 1915-1927) both end "nothing was written", for every path that raises the slug, C12 included.
  - Fix: keep the substring "nothing was written" and qualify it with the collections, which holds on every path (prepare gate, `run_apply` guard, C12 re-list, rollback refusal). Existing assertions that check that substring stay green: `deploy_apply.rs` about 7360 and 8166, `live_deploy_pair.rs` 1640, `error.rs` `deploy_pair_tags_refused_four_places` (about 3829).
  - README step 10 (about 3746-3752) goes from "staged on the host" straight to "the lock is taken" and does not name the pair re-list.
  - `pair_core_type_folder_made_after_the_plan_is_refused` (`deploy_apply.rs` about 8380-8425) asserts staging dropped, no commit, no lock, no scan and `failed_error_code(&rig)` (the one `result.json`, `failed_result` about 978). `options()` has `skip_gwbk: false` (about 491), so its run downloads a `.gwbk`.

**Hard rules (H1):**
- No production calls of any kind. No live rig is needed for this task.
- No production host names or IP addresses in any committed file or the SUMMARY (the repo is public). Describe hosts by role.
- No git worktrees. Run every cargo command sequentially on the main checkout. If phantom E0432 errors or vanished tests appear, rebuild in an isolated `CARGO_TARGET_DIR` before debugging code (`.claude/CLAUDE.md`).
- Stage by explicit path only. Never stage `.planning/config.json`, `.planning/research/.cache/`, the untracked `svc-ignition-deploy@*-uat4.ps1` script, `.planning/debug/tg-pg-table-check.md`, `.planning/STATE.md`, or this task's PLAN or SUMMARY (the orchestrator commits PLAN, SUMMARY and STATE).
- No co-author lines. No Cargo.toml or Cargo.lock change. No version bump. Do not push, open a PR or reply on the PR.
- No slug or exit-code change. `PLAN_GOLDEN*`, `RENDER_GOLDEN` and the fingerprint `GOLDEN` stay byte-identical to d5efc63.
- No quiet grep (`-q`) reading a pipe in any command you run under `set -o pipefail`. Use `grep ... >/dev/null` or `test -z "$(...)"`.
- Doc comments state the rule and cite the item (`quick 261104-ava, W1` / `N2` / ...). No narration. Match the surrounding comment density.
- Follow superpowers test-driven-development per code item and superpowers systematic-debugging for any failure. Never weaken an assertion, skip a test or mark one ignored to get green.
- Scratchpad: `/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad` (create if missing). Logs go there, never into the repo.
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1 (tracer): pass native paths to mklink /J in the conformance junction cases (W1)</name>
  <files>crates/ignition-core/tests/deploy_host_conformance.rs</files>
  <precondition>HEAD is d5efc63 on `feat/deploy-tags-core`, and `git status --short` shows only the H1 protected paths and this task directory as non-HEAD changes.</precondition>
  <read_first>
    - `crates/ignition-core/tests/deploy_host_conformance.rs`: `fn core_root` (about 202), `fn tree_names` (about 481), `async fn core_probe_walks_every_folder_on_a_touched_path` case (e) (about 1791-1816), `async fn ps_junction_target_dir_is_refused` (about 2742-2777). Line 12 already imports `Path`.
  </read_first>
  <behavior>
    - native_path_uses_backslashes_only (plain `#[test]`, no cfg, runs on macOS and Linux): `native_path(Path::new("C:\\Temp\\x/config/resources/core/ignition"))` is `C:\Temp\x\config\resources\core\ignition` (no `/` left), and an already native path comes back unchanged.
    - Case (e) on Windows: the junction is made, the probe refuses (`refused("a junctioned core\\ignition", ...)`), and `tree_names(outside.path())` is still `["tag-definition"]`. Both assertions unchanged.
    - `ps_junction_target_dir_is_refused` behaviour unchanged.
  </behavior>
  <action>
    Implements W1. It is the tracer: one test path, from the junction helper through `mklink` to the probe assertion, compiled for windows-msvc and pinned offline.

    1. **RED.**
       - Record for SUMMARY the CI evidence: run 41848470336, `cmd /C mklink /J` failing with `Invalid switch - "resources"`, both `conformance_local_` and `conformance_ps_core_probe_walks_every_folder_on_a_touched_path` panicking at the `mklink /J` assert (about line 1807).
       - Write `native_path_uses_backslashes_only` as specified, next to `tree_names`, before the helper exists. Run `cargo test -p ignition-core --test deploy_host_conformance native_path_uses_backslashes_only` and record the compile error (no `native_path` in scope).
    2. **Fix.**
       - Add a file-level `fn native_path(p: &Path) -> String` that returns the path's display string with every `/` replaced by `\`. Doc: mklink reads a `/` as the start of a switch, so a junction's paths go native (quick 261104-ava, W1). Not cfg-gated: the unit test uses it everywhere.
       - Add `#[cfg(windows)] fn make_junction(link: &Path, target: &Path)`. It runs `std::process::Command::new("cmd")` with `.args(["/C", "mklink", "/J"])`, then `.arg(native_path(link))` and `.arg(native_path(target))`, and asserts the status is a success, with the `Output` in the message.
       - Case (e): replace the inline `Command` block with `make_junction(&link, outside.path())`. Keep `std::fs::remove_dir(&link)`, the `refused(...)` call and the "nothing was written through the junction" assertion exactly as they are.
       - `ps_junction_target_dir_is_refused`: drop its local `native` closure and its inline `Command`, and call `make_junction(&link, outside.path())`. Its comment about native separators moves into `native_path`'s doc.
       - Change nothing else in the file. The live tests' junction sites need no change (see context).
    3. **Verify and commit.**
       - Run the verify command.
       - Commit `test(261104-ava): pass native paths to mklink in the conformance junction cases (W1)`, staging only `crates/ignition-core/tests/deploy_host_conformance.rs`.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && F=crates/ignition-core/tests/deploy_host_conformance.rs && cargo test -p ignition-core --test deploy_host_conformance native_path_uses_backslashes_only 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_host_conformance 2>&1 | grep -E '^test result: ok\.' && grep -E '^fn native_path\(p: &Path\) -> String' "$F" >/dev/null && grep -E '^fn make_junction\(link: &Path, target: &Path\)' "$F" >/dev/null && test "$(grep -cF 'mklink", "/J"' "$F")" = 1 && test -z "$(grep -nF '.arg("mklink")' "$F")" && test -z "$(grep -nE 'let native = \|' "$F")" && test "$(grep -cF 'make_junction(' "$F")" -ge 3 && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && test "$(git log --format=%s d5efc63..HEAD | grep -cE '^test\(261104-ava\): .*\(W1\)$')" = 1 && test -z "$(git diff --cached --name-only)" && echo T1-OK</automated>
  </verify>
  <done>`T1-OK` is printed.
- **Behaviour.** Both junction sites go through `make_junction`, which passes backslash-only paths. Case (e) still asserts refusal and that nothing was written through the junction.
- **Pinned.** `native_path_uses_backslashes_only` passes on macOS. The cfg(windows) code compiles and passes xwin clippy for windows-msvc.
- **Commit.** One `test(261104-ava): ... (W1)` commit touching only the conformance file, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: probe requested folders sftp omits, and charge unlisted long entries, in a Windows list_tree (N2, N3)</name>
  <files>crates/ignition-core/src/deploy/transport/sftp.rs</files>
  <precondition>Task 1's W1 commit is HEAD.</precondition>
  <read_first>
    - `crates/ignition-core/src/deploy/transport/sftp.rs`:
      - `struct Budget` (about 583), `struct Found` (about 626), `struct LongEntry` (about 637);
      - `async fn list_level_as` (about 743-797), for the cap and its wording;
      - `async fn cross_check` (about 930-1052) and `async fn enumerate_roots` (about 1057-1085);
      - `async fn discover` (about 1253-1345);
      - `async fn list_tree` (about 1767-1840) and the Windows block of `async fn read` (about 1849-1880).
    - Tests module: `struct Knobs` (about 3449-3515), the enumeration fake (about 3700-3790), the `hidden` check in the listing fake (about 3927), `fn windows_transport` (about 6635), and `windows_list_tree_is_checked_against_a_walk` (about 6797-6860) for the idiom.
  </read_first>
  <behavior>
    - windows_list_tree_refuses_a_type_folder_sftp_omits (N2, Windows fake, through `list_tree`):
      - Setup: `windows_fake()` with `windows_tree`; a file at `{WRES}/production/ignition/tag-definition/default/Deploy/tags.json`; `dirs` = `[("ignition","tag-definition"), ("ignition","tag-type-definition")]`. No `tag-type-definition` folder exists anywhere.
      - (a) Nothing hidden: `Ok`, one listing (`tag-definition`). The absent `tag-type-definition` is no error, and `enum_scripts` grew by exactly 1 (one PowerShell call for the found root and the missing folder together).
      - (b) `hidden` holds the type folder `{WRES}/production/ignition/tag-definition`: `Err` with code `transport_failed`, and the detail contains `did not return` and `production/ignition/tag-definition`. Before the fix: `Ok` with no listing.
      - (c) `hidden` holds `{WRES}/production/ignition` instead: the same refusal, naming the type folder.
    - windows_cross_check_counts_long_entries_sftp_omits (N3, Windows fake, `discover` then `cross_check` directly):
      - Setup: below `{WRES}/production/ignition/tag-definition`: two files, each with `enum_len_for` 262 and in `hidden`; one empty folder with `enum_len_for` 262 and in `hidden`; one more file with `enum_len_for` 262 and not hidden (sftp returns it).
      - Build the type folder chain as `list_tree` does, then `discover` with a fresh `Budget`.
      - (a) `budget.entries = READ_MAX_ENTRIES - 3`, then `cross_check(&found, &[], &mut budget)`: `Ok`, and `budget.entries == READ_MAX_ENTRIES`. Exactly the three hidden long entries are charged; the listed long file is not.
      - (b) `budget.entries = READ_MAX_ENTRIES - 2`: `Err` `transport_failed`, and the detail contains `more than {READ_MAX_ENTRIES} entries`.
      - Before the charge exists: (a) leaves `budget.entries` at `READ_MAX_ENTRIES - 3`, and (b) returns `Ok`.
    - Every existing sftp test stays green unchanged, `windows_list_tree_is_checked_against_a_walk` and the Windows read tests included.
  </behavior>
  <action>
    Implements N2 and N3 in one commit, because both change `cross_check`'s signature and body.

    1. **N2 RED.** Write `windows_list_tree_refuses_a_type_folder_sftp_omits` as specified. Run `cargo test -p ignition-core --lib windows_list_tree_refuses_a_type_folder_sftp_omits` and record the failing case (b), `Ok` with no listing, for SUMMARY. If it passes, N2 does not reproduce: stop the N2 part and record it.
    2. **Thread the parameters (no new behaviour yet).**
       - Change `cross_check` to take `found: &Found`, `absent: &[Vec<String>]` and `budget: &mut Budget`, and return `Result<Vec<LongEntry>, CoreError>` as now.
       - `read` calls `self.cross_check(&found, &[], &mut budget)` with its existing budget.
       - `list_tree` computes `absent`: the requested chains (its `chains` set) that `found.roots` lacks. It keeps its `budget` and calls `self.cross_check(&found, &absent, &mut budget)` on Windows.
       - The body ignores both new parameters for now. Build green.
    3. **N3 RED.** Write `windows_cross_check_counts_long_entries_sftp_omits` as specified. Run it and record the failing assertion (budget unchanged in (a), `Ok` in (b)).
    4. **Fix N2** in `cross_check`:
       - Enumerate the native paths of `found.roots` followed by `absent` in the one `enumerate_roots` call. Split the results back by position.
       - The early return becomes: nothing to enumerate when both `found.roots` and `absent` are empty.
       - For each absent chain the host finds (`Some`), return `self.fail` with: `<chain joined by "/"> exists on the host but the sftp listing did not return it (the sftp server drops paths of 260 characters or more, or the folder was made while it was read)`. An absent chain the host does not find (`None`) is fine.
       - Doc: a requested folder the listing omits is refused like any omitted entry, because a names-only listing has no unreadable list (quick 261104-ava, N2; the WR-03 rule of `cross_check_names`). Note in the doc that `read` passes no missing roots.
    5. **Fix N3** in `cross_check`, per found root:
       - Build that root's sftp file set and folder set before the long-path filter, so each lookup is a set lookup, not a scan.
       - Count the root's long entries that the sftp listing did not return: a file whose relative path is not in the root's sftp file set, a folder not in its sftp folder set.
       - Add the count to `budget.entries`. When `budget.entries > READ_MAX_ENTRIES`, return `self.fail(format!("more than {READ_MAX_ENTRIES} entries under the data directory (at {shown_root})"))`, the `list_level_as` wording.
       - Keep the existing missing, extra and size comparisons exactly as they are; they still exclude long paths.
       - Doc: long entries sftp did return were charged by `list_level`; the others are charged here, so the cap holds on a Windows host too (quick 261104-ava, N3).
    6. **Green.** Both new tests pass, and every existing `transport::sftp` test passes unchanged. If an existing Windows test now fails, debug it first (superpowers systematic-debugging). Change its expectation only if the old one encoded the N2 or N3 gap, and record it as a deviation.
    7. **Verify and commit.** Run the verify command. Commit `fix(261104-ava): probe requested folders sftp omits and count long entries in a Windows list_tree (N2, N3)`, staging only `crates/ignition-core/src/deploy/transport/sftp.rs`.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && F=crates/ignition-core/src/deploy/transport/sftp.rs && cargo test -p ignition-core --lib windows_list_tree_refuses_a_type_folder_sftp_omits 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib windows_cross_check_counts_long_entries_sftp_omits 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib windows_list_tree_is_checked_against_a_walk 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib transport::sftp 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --test deploy_plan --test deploy_apply --test deploy_pair --test deploy_verify --test deploy_transport 2>&1 | grep -E '^test result' && grep -F 'absent: &[Vec<String>],' "$F" >/dev/null && grep -F 'self.cross_check(&found, &absent, &mut budget)' "$F" >/dev/null && grep -F 'self.cross_check(&found, &[], &mut budget)' "$F" >/dev/null && test "$(grep -cF 'self.cross_check(' "$F")" = 2 && grep -F 'exists on the host but the sftp listing did not return it' "$F" >/dev/null && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && test "$(git log --format=%s d5efc63..HEAD | grep -cE '^fix\(261104-ava\): .*\(N2, N3\)$')" = 1 && test -z "$(git diff --cached --name-only)" && echo T2-OK</automated>
  </verify>
  <done>`T2-OK` is printed.
- **N2.** A requested type folder that the Windows sftp listing omits, directly or below an omitted `ignition` folder, is `transport_failed` naming it. A folder absent on the host is no error, and the enumeration is still one PowerShell call.
- **N3.** Long entries sftp did not return count against `READ_MAX_ENTRIES` in both `read` and `list_tree`, with the `list_level` wording past the cap. Listed long entries are not charged twice.
- **Unchanged.** Every existing sftp, plan, apply, pair, verify and transport test passes.
- **Commit.** One `fix(261104-ava): ... (N2, N3)` commit touching only `sftp.rs`.</done>
</task>

<task type="auto">
  <name>Task 3: README and doc text for N1 and N4, the C12 test pin, SUMMARY and the full gate</name>
  <files>README.md, crates/ignition-core/src/deploy/apply/mod.rs, crates/ignition-core/src/error.rs, crates/ignition-core/tests/deploy_apply.rs, .planning/quick/261104-ava-pr35-round2-win-ci-sftp/261104-ava-SUMMARY.md</files>
  <precondition>Task 2's N2/N3 commit is HEAD.</precondition>
  <read_first>
    - `README.md`: the `deploy apply` Commands row (line 585, one long line); `#### Tags and redundant pairs` from `- **What is still refused.**` to `- **Designer edits are drift.**` (about 2684-2710), plus the `**deploy verify**` bullet after it (about 2716-2726) for wording; the `deploy plan` paragraph `**Redundant pairs.** For a target with` (about 3096-3105); apply step `5. For a target with` (about 3724-3732) and step `10. The run directory is created` (about 3746-3752); `**The run directory.**` (about 3944-3950).
    - `crates/ignition-core/src/deploy/apply/mod.rs`: the module doc branch table (about 70-80); `drive` from the `prepare` call to `HeldLock::acquire` (about 1300-1340).
    - `crates/ignition-core/src/error.rs`: `fn deploy_pair_tags_refused_text` (about 1129-1137), the `DeployPairTagsRefused` hint (about 1915-1927), `fn deploy_pair_tags_refused_four_places` (about 3829-3858).
    - `crates/ignition-core/tests/deploy_apply.rs`: `fn failed_result` (about 978) and `async fn pair_core_type_folder_made_after_the_plan_is_refused` (about 8375-8425).
    - `.planning/quick/261104-7cy-coderabbit-review-fixes-pr35/261104-7cy-SUMMARY.md`, for the SUMMARY layout.
  </read_first>
  <action>
    1. **N1** (README, docs only; the behaviour is already pinned, see context).
       - In the `What is still refused` bullet, add one exception for an `unmanaged` tag unit (live in the mode collection, neither declared nor listed by the ledger). The deploy never deletes what it does not own, so such a unit is not refused, `deploy plan` does not name it, and its files, which the plan reads, are not residue of the folder listing. Its mode files still keep the backup from syncing: `deploy verify` warns about them (the `deploy verify` bullet below), and they are removed by hand on the master (runbook T6e).
       - In the `deploy apply` Commands row (line 585), qualify the `deploy_pair_tags_refused` clause the same way, in a few words with the word `unmanaged`, linking to Tags and redundant pairs as the row already does.
       - Add the same short qualifier to the `deploy plan` `**Redundant pairs.** For a target with` paragraph (its warning does not name an `unmanaged` unit) and to apply step 5.
       - Keep every other sentence. The `deploy verify` bullet already says the warning covers other units' mode files.
    2. **N4** (README, `apply/mod.rs`, `error.rs`, one test pin).
       - README, `What is still refused`: rewrite the end of the sentence that begins `deploy apply` lists the folders again after staging. It should now say that no collection file is written and no lock is taken; that the run's staging and snapshot on the host are dropped; and that the run directory stays, with the downloaded `.gwbk` (unless `--skip-gwbk`) and the `result.json` that records the refusal. Name the run directory's other files only if `drive` / `run_apply` confirm them.
       - README step 10: between staging and the lock, add the pair re-list. On a pair, the mode tag type folders are listed again, and residue made after the plan is `deploy_pair_tags_refused`, with the same outcome as above.
       - `apply/mod.rs` branch table: in the re-list row (`a pair's mode tag type folders, listed again after staging`), add that the run directory, its `.gwbk` and `result.json` stay. In the lock-refused row, change its closing claim to `nothing was written into a collection` (the run directory exists on that branch too). Keep the table's other rows unchanged.
       - `error.rs`: the `deploy_pair_tags_refused_text` message's last clause becomes `nothing was written to the {mode} or core collection`. The hint's last clause becomes `nothing was written to the mode or core collection`. Keep each phrase on one source line.
       - In `deploy_pair_tags_refused_four_places`, replace the message's `"nothing was written"` part with the literal `"nothing was written to the production or core collection"`. Replace the hint's `"nothing was written"` check with the literal `"nothing was written to the mode or core collection"`. Slug, exit code and the other parts are unchanged.
       - `deploy_apply.rs`, `pair_core_type_folder_made_after_the_plan_is_refused`: add assertions that the run directory `failed_result` reads still holds `result.json` and exactly one file ending `.gwbk`. Locate it the way `failed_result` does. Extend the doc comment: the run directory, its `.gwbk` and `result.json` stay; only the host's staging and snapshot go (quick 261104-ava, N4).
       - Run `cargo test -p ignition-core --lib deploy_pair_tags_refused_four_places`, `cargo test -p ignition-core --test deploy_apply pair_core_`, `cargo test -p ignition-core --lib readme_exit_table_agreement` and `cargo test -p ignition-cli --test deploy_family_coverage`.
    3. **N1 pins.** Run `cargo test -p ignition-core --lib pair_tags_written_or_kept_in_the_mode_collection_are_refused_on_a_pair_only`, `cargo test -p ignition-core --test deploy_plan pair_mode_tag_dirs_names_only_folders_with_no_read_file` and `cargo test -p ignition-core --test deploy_pair pair_verify_warns_on_mode_tag_files`, and cite them in SUMMARY.
    4. **Commit.** Commit `docs(261104-ava): document the unmanaged tag unit exception and what a late re-list refusal leaves (N1, N4)`. Stage README.md, `apply/mod.rs`, `error.rs` and `deploy_apply.rs` by explicit path.
    5. **SUMMARY** (`261104-ava-SUMMARY.md` in this task directory; never committed by you). Frontmatter as in 7cy (phase `quick-261104-ava`, plan 01, status, key-files, decisions, metrics). Body:
       - `## Findings`: exactly one line each, in the form `W1: fixed`, `N1: fixed`, `N2: fixed`, `N3: fixed`, `N4: fixed` (or `<id>: not reproduced (<reason>)`). Under each: the RED evidence (W1: the CI run and its error text plus the helper test's compile error; N2 and N3: the failing test names and messages), what changed, and the commit sha.
       - Under W1: the junction sites checked, with no change needed in the live tests. Behaviour runs only on the Windows CI leg after a push, which this task does not do.
       - The **N2 decision** (fail with `transport_failed`, not list, and why) and the **read() observation** (scoped roots sftp omits are not probed; unchanged, outside N2).
       - Under N4: what stays and what goes, and the message and hint qualifier (the substring kept, slug and exit unchanged).
       - **Windows CI items**: case (e) of `conformance_local_` and `conformance_ps_core_probe_walks_every_folder_on_a_touched_path`, and `conformance_ps_junction_target_dir_is_refused` if that is its test name.
       - Deviations, Hygiene, and the gate tokens `T1-OK`, `T2-OK` and `FULL-GATE-OK`.
    6. **Full gate.** Run the verify command. If its only failure is the known wall-clock test `wait_restart_witnessed_path_golden` (`crates/ignition-cli/tests/contract_restart_wait.rs`), run that test alone, run the gate once more, and record a deviation. Change nothing in that file.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && SP=/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad && mkdir -p "$SP" && B=d5efc63 && S=.planning/quick/261104-ava-pr35-round2-win-ci-sftp/261104-ava-SUMMARY.md && M=crates/ignition-core/src/deploy/apply/mod.rs && E=crates/ignition-core/src/error.rs && ( for c in W1 N1 N2 N3 N4; do test "$(grep -cE "^${c}: (fixed|not reproduced)" "$S")" = 1 || exit 1; done ) && test -z "$(grep -E '(^|[^0-9.])([0-9]{1,3}\.){3}[0-9]{1,3}([^0-9.]|$)' "$S" | grep -vE '127\.0\.0\.1|0\.0\.0\.0|192\.0\.2\.')" && test -z "$(grep -E '(^|[^0-9.])10\.[0-9]{1,3}\.' "$S")" && test -z "$(grep -iE '(^|[^a-z0-9])[a-z]{4,}vm[0-9]{2}' "$S")" && grep -E '^\| `ign deploy apply --env' README.md | grep -F 'unmanaged' >/dev/null && sed -n '/^- \*\*What is still refused\.\*\*/,/^- \*\*Designer edits/p' README.md | grep -F 'unmanaged' >/dev/null && sed -n '/^- \*\*What is still refused\.\*\*/,/^- \*\*Designer edits/p' README.md | grep -F 'result.json' >/dev/null && sed -n '/^- \*\*What is still refused\.\*\*/,/^- \*\*Designer edits/p' README.md | grep -F '.gwbk' >/dev/null && test -z "$(sed -n '/^- \*\*What is still refused\.\*\*/,/^- \*\*Designer edits/p' README.md | grep -F 'with nothing written')" && sed -n '/^\*\*Redundant pairs\.\*\* For a target with/,/^$/p' README.md | grep -F 'unmanaged' >/dev/null && sed -n '/^5\. For a target with `nodes.backup`/,/^6\. /p' README.md | grep -F 'unmanaged' >/dev/null && sed -n '/^10\. The run directory is created/,/^$/p' README.md | grep -F 'deploy_pair_tags_refused' >/dev/null && grep -E '^//! \| a pair.s mode tag type folders, listed again after staging.*result\.json' "$M" >/dev/null && grep -E '^//! \| the lock is refused .*nothing was written into a collection \|$' "$M" >/dev/null && grep -F 'nothing was written to the {mode} or core collection' "$E" >/dev/null && grep -F 'nothing was written to the mode or core collection' "$E" >/dev/null && grep -F '"nothing was written to the production or core collection"' "$E" >/dev/null && sed -n '/async fn pair_core_type_folder_made_after_the_plan_is_refused/,/^}/p' crates/ignition-core/tests/deploy_apply.rs | grep -F 'gwbk' >/dev/null && cargo test -p ignition-core --lib deploy_pair_tags_refused_four_places 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_apply pair_core_type_folder_made_after_the_plan_is_refused 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib pair_tags_written_or_kept_in_the_mode_collection_are_refused_on_a_pair_only 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_plan pair_mode_tag_dirs_names_only_folders_with_no_read_file 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_pair pair_verify_warns_on_mode_tag_files 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib readme_exit_table_agreement 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-cli --test deploy_family_coverage 2>&1 | grep -E '^test result: ok\.' && test "$(git rev-list --count $B..HEAD)" = 3 && ( for m in '(W1)' '(N2, N3)' '(N1, N4)'; do test "$(git log --format=%s $B..HEAD | grep -cF "$m")" = 1 || exit 1; done ) && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && test -z "$(git diff -U0 $B -- crates/ignition-core/tests/deploy_plan.rs crates/ignition-core/src/deploy/apply/fingerprint.rs crates/ignition-cli/tests/contract_deploy.rs | grep -vE '^(\+\+\+|---) ' | grep -E '^[-+].*(GOLDEN|[0-9a-f]{64})')" && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | tee "$SP/261104-ava-workspace.log" | grep -E '^test result' | tail -3 && test -z "$(grep -E '^test result: FAILED' "$SP/261104-ava-workspace.log")" && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && git diff --quiet $B -- Cargo.toml Cargo.lock crates/ignition-cli/Cargo.toml crates/ignition-core/Cargo.toml crates/ignition-tui/Cargo.toml && test -z "$(git log --name-only --format= $B..HEAD | grep -E '^\.planning/config\.json$|^\.planning/research/\.cache/|-uat4\.ps1$|^\.planning/debug/tg-pg-table-check\.md$|^\.planning/STATE\.md$|261104-ava-(PLAN|SUMMARY)\.md$')" && test -z "$(git diff $B..HEAD | grep -E '^\+' | grep -vE '127\.0\.0\.1|0\.0\.0\.0|192\.0\.2\.' | grep -E '(^|[^0-9.])([0-9]{1,3}\.){3}[0-9]{1,3}([^0-9.]|$)')" && test -z "$(git log --format=%B $B..HEAD | grep -i 'co-authored-by')" && test -z "$(git diff $B..HEAD -- crates | grep -E '^\+' | grep -F '#[ignore')" && git diff --quiet -- .planning/STATE.md && test -z "$(git diff --cached --name-only)" && echo FULL-GATE-OK</automated>
  </verify>
  <done>`FULL-GATE-OK` is printed.
- **N1.** Four README places name the `unmanaged` exception, consistent with the three pins, which pass.
- **N4.** The README C12 sentence and step 10, the branch table and the error message and hint say what a late re-list refusal leaves. The C12 test pins the `.gwbk` and `result.json`. Slug and exit code are unchanged.
- **SUMMARY.** One line per item (W1, N1-N4) with RED evidence and commits, the N2 decision and read() observation, and the Windows CI items. It passes the hygiene greps.
- **Full gate.** fmt, workspace clippy, `cargo test --workspace` and windows-msvc xwin clippy are green. Goldens are unchanged. There are exactly three commits since d5efc63, no Cargo change, no protected or planning-state file in a commit, no IP literal, no co-author line, no new ignored test, and nothing staged.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Windows sftp server -> `list_tree` / `read` | the sftp listing silently drops long paths; the PowerShell walk is the completeness proof |
| live mode collection -> pair residue check | a type folder the reader cannot see decides whether a pair deploy is refused |
| docs and error text -> operator | an operator acts on what the README and the hint say a refusal left behind |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-ava-01 | Denial of service (pair wedge) | `SftpTransport::list_tree` on a Windows host, a requested type folder sftp omits (N2) | high | mitigate | Every requested folder sftp did not return is enumerated in the same PowerShell call. One the host finds is `transport_failed`, so plan, verify and the C12 re-list fail closed. Pinned by `windows_list_tree_refuses_a_type_folder_sftp_omits` |
| T-ava-02 | Denial of service (resource) | `cross_check` long-entry growth (N3) | medium | mitigate | Unlisted long entries are charged to the read's `Budget`, and the read fails past `READ_MAX_ENTRIES` with the `list_level` wording. Pinned by `windows_cross_check_counts_long_entries_sftp_omits` |
| T-ava-03 | Tampering / Elevation | a junctioned `core\ignition` (C6) not exercised on Windows CI because the test setup failed (W1) | medium | mitigate | `make_junction` passes native paths, so case (e) runs its refusal assertion on the Windows leg. `native_path` is pinned offline |
| T-ava-04 | Repudiation (misleading record) | README, branch table and hint claiming nothing was written after a C12 refusal (N4) | low | mitigate | The text names what stays (run directory, `.gwbk`, `result.json`) and what is dropped. The C12 test asserts the `.gwbk` and `result.json` remain |
| T-ava-05 | Repudiation (misleading docs) | README implying every unledgered mode tag file is refused (N1) | low | mitigate | The `unmanaged` exception is documented in four places, and the pins that encode it are run in the gate |
| T-ava-06 | Information disclosure | host names or IPs in committed files or the SUMMARY | medium | mitigate | The full gate greps the SUMMARY and the diff for IP literals, private ranges and VM-style names. The protected script is never staged |
| T-ava-SC | Tampering | package installs | low | accept | No new crate. The full gate asserts no Cargo.toml or Cargo.lock change since d5efc63 |
</threat_model>

<verification>
- **Task 1:** `native_path_uses_backslashes_only` passes, the conformance suite is green on macOS, xwin clippy compiles the cfg(windows) junction code, and there is one W1 commit (T1-OK).
- **Task 2:** both new Windows sftp tests and every existing sftp test pass, `cross_check` has the two call sites with the new parameters, and there is one N2/N3 commit (T2-OK).
- **Task 3:** the README, branch table and error-text greps pass, the N1 pins and the C12 pin pass, SUMMARY has one line per item, and the full gate is green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` (FULL-GATE-OK).
- **Scope:** no worktree, version bump, slug or exit-code change, push, PR reply or production call. Protected files, STATE.md, this PLAN and the SUMMARY stay out of the executor's commits.
</verification>

<success_criteria>
- The Windows CI leg's junction case reaches its refusal assertion instead of failing in `mklink`.
- A Windows pair host cannot hide a mode tag type folder from the residue check, and a long-path tree cannot exceed the read cap.
- The README says an `unmanaged` tag unit is the exception to the pair refusal, and says what a late re-list refusal leaves.
- Every item W1, N1-N4 has a SUMMARY line with its evidence and commit, and the gates are green.
</success_criteria>

<output>
Create `.planning/quick/261104-ava-pr35-round2-win-ci-sftp/261104-ava-SUMMARY.md` (Task 3, step 5). Do not commit it; the orchestrator commits PLAN, SUMMARY and STATE.md.
</output>
