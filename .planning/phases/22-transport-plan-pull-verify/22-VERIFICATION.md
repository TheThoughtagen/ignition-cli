---
phase: 22-transport-plan-pull-verify
verified: 2026-10-16T00:30:00Z
status: human_needed
score: 5/5 roadmap success criteria verified (10/10 requirement IDs satisfied); UAT gaps G-22-1 and G-22-2 closed in code and automated evidence
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 5/5 roadmap success criteria verified (10/10 requirement IDs satisfied)
  trigger: "22-UAT.md found G-22-1 (CI red, fail-fast hid cfg(windows)) and G-22-2 (pulled tree did not verify green: one-sided null strip); gap plans 22-15 and 22-16"
  gaps_closed:
    - "G-22-1: CI green on all four jobs incl. windows-latest at ab8608a (run 38838954722)"
    - "G-22-2: symmetric N1 null normalisation in verify read_back (verify.rs strip_nulls on file and REST side)"
  gaps_remaining: []
  regressions: []
gaps: []
deferred:
  - truth: "Hold budget (505 s without a Defender exclusion, 699 s with one) exceeds the 300 s ceiling"
    addressed_in: "Phase 23"
    evidence: "Phase 23 SC-3 'Apply writes inside the scan-lock within the hold budget measured in Phase 22'; STATE.md carries it as an OPEN blocker 'Phase 23 planning must open with it'. Phase 22's gate asks for the budget to be measured and recorded, which it is."
human_verification:
  - test: "Re-run 22-UAT.md test 2 (G-22-2) on the user's own rig: `ign deploy pull`, then `ign deploy verify` on the UNMODIFIED pulled tree, with the ign-cli WebDev bundle installed and one Good `verify.tag_probes[]` entry"
    expected: "exit 0, data.ok true, read_back passes with no finding on ignition/cobranding, probes passes; then adding a Bad probe still gives tag_probe_not_good and exit 6"
    why_human: "The UAT result is a human-recorded state. Agent-run live evidence exists (22-15-SUMMARY.md: pull_then_verify_is_green_live and the installed binary, green exit 0 with 28 resources, red exit 6 after a one-member edit) but 22-UAT.md re-test belongs to /gsd-verify-work and was deliberately not marked passed here."
  - test: "Re-run 22-UAT.md test 1 (G-22-1) acceptance in /gsd-verify-work"
    expected: "Recorded as passed against CI run 38838954722 (all four jobs success on ab8608a)"
    why_human: "Evidence is verified below, but the UAT file state change is owned by /gsd-verify-work."
---

# Phase 22: Transport, Plan, Pull & Verify - Verification Report (re-verification)

**Phase Goal:** A user can see exactly what a deploy would change on the live master, bootstrap a declared tree from a running gateway, and verify a gateway against its tree, all read-only, over a transport whose write half is built and tested here but not yet called by any verb.
**Verified:** 2026-10-16 at HEAD dbf8369 (origin/main = ab8608a; HEAD is 3 docs-only commits ahead)
**Status:** human_needed. No gaps. Both UAT gaps are closed in code and automated evidence. What remains is the human UAT re-test (owned by `/gsd-verify-work`) and one deferred item for Phase 23.
**Re-verification:** Yes. Previous status human_needed; UAT then raised G-22-1 and G-22-2.

## Gap closure verification

### G-22-2: pulled tree did not verify green (CLOSED)

| Check | Evidence |
|-------|----------|
| Fix is in the code, not just the summary | `crates/ignition-core/src/deploy/verify.rs`: new `strip_nulls` (line 652, recursive over objects and arrays). `check_read_back` applies it to BOTH operands: `Compare::Exact` strips file and REST (615-616), `NameKeyedSubset` strips both (619-620). `diff_attributes` (782) strips both compared values (790) but does the key lookup on the RAW REST map (`rest.get(key)`), so a REST `null` for a declared key is still compared and still differs. |
| One-sided null cannot hide a real difference | After stripping both sides, a null on one side vs a value on the other leaves the key on one side only, which `diff_exact`/`diff_subset` report. Guard tests assert both directions fail and that the failing finding never contains the value. |
| Regression tests exist and pass in my run | `crates/ignition-core/tests/deploy_verify.rs`: `read_back_n1_null_equals_absent_on_both_sides`, `read_back_n1_cobranding_rest_nulls_matching_the_file_pass`, `read_back_n1_rest_null_against_a_file_value_fails`, `read_back_n1_file_null_against_a_rest_value_fails`, `read_back_n2_rest_nulls_are_dropped`, `read_back_n3_rest_nulls_are_dropped`, `verify_green_when_rest_echoes_file_nulls` (mock gateway that keeps nulls via `rest_config_raw`, with a `has_null` shape precondition so it cannot pass vacuously). `crates/ignition-cli/tests/contract_deploy.rs`: `verify_cli_green_when_rest_echoes_file_nulls` (binary level, `rest_keeps_nulls` parameter). All `ok` in the local run. |
| The test gap that hid the defect is closed | Previously the mock `rest_config` helpers stripped nulls. A raw-null helper now exists, and `live_deploy_plan.rs::pull_then_verify_is_green_live` (line 537) runs pull then verify on the pulled tree. It is `#[ignore]` (needs the rig, `IGNITION_LIVE_DEPLOY=1`), so it did not run here. |
| Live evidence recorded | `22-15-SUMMARY.md` and `~/.cache/ign-deploy-rig/g22-2-live.log` (read directly): `PASS pull_then_verify_is_green_live`, `pulled: ok true slug None exit None resources_checked 28`, then after a one-member edit `edited: ok false slug Some("deploy_verify_failed") exit Some(6)`; `PASS pull_render_plan_is_empty_live`; `PASS plan_takes_no_lock_live`; `PASS verify_green_then_red_live` (green 2 resources, red exit 6). The summary also records the installed `ign 1.5.0` binary: `deploy pull` exit 0 then `deploy verify` exit 0 with 28 resources, and exit 6 after the edit. I did not re-run live tests (no rig started, no gateway writes). |
| Docs corrected | `README.md:2935-2946` now states nulls are dropped from both sides and a one-sided null still differs; the "pulled tree compares equal" claim now matches behaviour. `22-LIVE-CAPTURES.md:8176` has an own `ignition/cobranding` row and `:8184` a dated amendment (G-22-2, plan 22-15). `verify.rs` module doc matches. |

### G-22-1: CI red, cfg(windows) never compiled (CLOSED)

| Check | Evidence |
|-------|----------|
| CI run | `gh run view 38838954722 --json conclusion,headSha,jobs`: conclusion `success`, headSha `ab8608a3d64229eb12cd25bcd5092cd76c23b94b`, jobs `check (ubuntu-latest)` success, `check (macos-latest)` success, `check (windows-latest)` success, `keyring-smoke` success. |
| On origin/main | `git branch -r --contains ab8608a` lists `origin/main`; `git log origin/main -1` is `ab8608a`. Local HEAD dbf8369 is 3 docs-only commits ahead (`b7b3104`, `025262c`, `dbf8369`). |
| What it took | Six CI rounds (22-16-SUMMARY.md): the two borrows named in the gap, three more Windows-only clippy findings, one product fix (`770b38f`, winps ENUMERATE built relative paths with `FullName.Substring`, which shifted when .NET spells the root differently; production uses `\\?\` so live runs never hit it), and four test-premise fixes (inode reuse on ubuntu ext4, CRLF README on Windows, `\` to `/` SftpOp local paths, Windows ssh `NUL` option spellings). The 22-REVIEW-GAPS.md review traced the product changes and found no defect. The Windows leg now compiles, lints and runs the `cfg(windows)` code. |
| This closes the earlier human item 1 | The Windows CI leg is now run evidence, not an open question. |

## Goal Achievement

### Observable Truths (ROADMAP success criteria): regression check

Previously verified with full evidence; re-checked here against the changed files (12 files, `395f8ee..HEAD`) and the full suite.

| # | Truth | Status | Re-verification note |
|---|-------|--------|----------------------|
| 1 | Read transport (system OpenSSH, local FS), hardened, write half built/tested and unreachable from any verb | VERIFIED | `remote.rs`, `local.rs`, `winps.rs`, `tree.rs` were touched by 22-16. The write-half guard `write_half_unreachable` (deploy_transport.rs) is in the green run. The Windows leg now also passes in CI. ENUMERATE fix reviewed (no injection surface; names are data). Live Windows-host evidence (22-13) stands. |
| 2 | `plan` zero writes, no lock, exact delete set, relations and flags | VERIFIED | Files unchanged by gap plans; `deploy_plan` suite green. |
| 3 | `--base` drift; `pull` bootstraps; `render` then `plan` is empty | VERIFIED | Unchanged; `pull_render_plan_is_empty_live` re-run live in 22-15 (91 files written, 7 skipped, plan empty). |
| 4 | `verify` six checks, exit 6 on red, `tag_probes[]` | VERIFIED | Changed by G-22-2 only in null handling. UAT test 2 (human-reported) already showed probes behaving as specified live (Good passes; Bad gives `tag_probe_not_good`, exit 6). The pulled-tree-equal behaviour that failed in UAT is now fixed and covered. |
| 5 | TUI rows, README, exit table, MCP catalog, secret canary | VERIFIED | `deploy_family_coverage.rs` changed (`lines()` for CRLF, stricter not weaker) and green. |

**Score:** 5/5 truths verified. `behavior_unverified` is 0.

### Independent test run

`CARGO_TARGET_DIR=$HOME/Library/Caches/cargo-target-ign-cli-stable PATH=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH cargo test --workspace` at HEAD dbf8369 on macOS: **2151 passed, 0 failed, 51 ignored**, zero "FAILED" or "panicked" lines. This matches 22-16's final figure (2151). The previous verification's 2130 / 50 plus the gap-plan tests accounts for the growth; the extra ignored test is `pull_then_verify_is_green_live`, which is listed as `ignored, live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1`. The ignored live tests were not re-run.

### Requirements Coverage

Unchanged and re-confirmed against REQUIREMENTS.md. Every ID is claimed by a PLAN frontmatter `requirements` field (22-15 adds DPLN-07 and DPLN-06; 22-16 adds DPLN-01), and REQUIREMENTS.md maps exactly these 10 IDs to Phase 22. No orphans.

| Requirement | Status | Evidence |
|-------------|--------|----------|
| DPLN-01 | SATISFIED | SC-1; Windows CI leg now green (G-22-1). |
| DPLN-02, 03, 04 | SATISFIED | SC-2. |
| DPLN-05 | SATISFIED | SC-3 `--base`. |
| DPLN-06 | SATISFIED | Live pull then plan empty (22-09, 22-13, 22-15). The box is now ticked (`[x]`) and the traceability row reads Complete, so the earlier bookkeeping recommendation is resolved. |
| DPLN-07 | SATISFIED | SC-4; the pulled-tree false-red (G-22-2) is fixed, so "every declared resource reads back equal" now holds for a pulled tree. |
| DPLN-08 | SATISFIED | Hermetic tests plus the UAT-reported live probe run (Good pass, Bad fail exit 6). |
| DPLN-09 | SATISFIED | SC-4 exit 6, live in 22-10 and 22-15. |
| DSURF-04 | SATISFIED | README dated exception. |

### Anti-Patterns Found

Scanned the added lines of `git diff 395f8ee HEAD -- crates README.md` for `TBD|FIXME|XXX|TODO|HACK|todo!|unimplemented!`: none. No stubs or placeholder returns on the changed product paths. The only `#[ignore]` tests are live gates with documented guards.

### Advisory (from 22-REVIEW-GAPS.md; not blockers)

- WR-01: `find_cobranding` in `live_deploy_plan.rs` returns the first match in unspecified `read_dir` order. If a pull ever wrote both a core and a mode-collection copy, the live gate's repo-side edit could target the non-effective one and be flaky. Live-only, `#[ignore]`d, did not affect the recorded green run. Worth tightening before the live gate is relied on repeatedly.
- IN-01: the verify.rs module doc says `database-translator` omits null members, the test-helper docs cite the tracer translator as a null-echoing example, and the README names only cobranding. The three should be reconciled against live evidence. No effect on behaviour.

## Warnings (carried forward, non-blocking)

1. **Hold budget above the ceiling (deferred to Phase 23).** 505 s (no Defender exclusion) and 699 s (exclusion) against a 300 s ceiling; recorded as an open Phase 23 blocker. Phase 22's gate asks for it to be measured and recorded, which it is.
2. The 22-15 deviation note: a binary live gate must run before `verify_green_then_red_live` (that test leaves an in-memory `POSTGRES` override until the next scan) or the rig must be rescanned. Relevant to anyone re-running the live gate by hand.
3. The Windows host used for live proof is an Azure VM (Win32-OpenSSH 8.1) under user-approved amendment A-01/A-02.
4. Verify read-back can still false-red a hand-written, minimal `config.json` for types where the gateway fills defaults (for example `database-connection`); a pulled tree compares equal. Documented in the README.

## Human Verification Required

### 1. UAT test 2 re-test (G-22-2)

**Test:** On the local `ign-deploy-rig` (or any 8.3.x gateway with the ign-cli WebDev bundle installed), run `ign deploy pull`, then `ign deploy verify` on the unmodified pulled tree with a Good `verify.tag_probes[]` entry. Then add a Bad probe.
**Expected:** exit 0 and `data.ok` true with read_back passing (no `ignition/cobranding` finding) and probes passing; with the Bad probe, `tag_probe_not_good` and exit 6.
**Why human:** Agent-run live evidence exists (22-15-SUMMARY.md, g22-2-live.log), but 22-UAT.md state is owned by `/gsd-verify-work`; it was not marked passed here.

### 2. UAT test 1 re-test (G-22-1)

**Test:** Record UAT test 1 against CI run 38838954722.
**Expected:** passed (all four jobs success on ab8608a, which I confirmed with `gh`).
**Why human:** The UAT file state change belongs to `/gsd-verify-work`.

## Gaps Summary

None. G-22-2 is closed in the code (symmetric `strip_nulls` on both operands, N3 lookup kept on the raw map), in six named tests that pass, and in recorded live evidence. G-22-1 is closed by CI run 38838954722, which I confirmed with `gh` as `success` on all four jobs at `ab8608a`, present on `origin/main`. The local `cargo test --workspace` run is 2151 passed, 0 failed, 51 ignored. The five roadmap success criteria and ten requirement IDs hold. Status stays `human_needed` only because the UAT re-test is a human step that this verifier does not perform.

---

_Verified: 2026-10-16T00:30:00Z_
_Verifier: Claude (gsd-verifier)_
