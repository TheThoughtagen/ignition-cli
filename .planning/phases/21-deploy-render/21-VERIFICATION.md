---
phase: 21-deploy-render
verified: 2026-10-13T17:30:00Z
status: passed
score: 4/5 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification: false
human_verification:

  - test: "Push local main (83 commits ahead of origin/main, head 039a552) or a PR branch carrying crates/ignition-core/tests/deploy_corpus.rs, and wait for the CI workflow (.github/workflows/ci.yml) to finish on ubuntu-latest, macos-latest and windows-latest"
    expected: "All three legs green, in particular deploy_corpus (6 tests), deploy_render (61), deploy_fixtures (9) and contract_deploy (9). Then record the run id and flip DRND-05 to Complete in REQUIREMENTS.md"
    why_human: "Pushing is an outward action, and the DRND-05 and ROADMAP SC3 wording requires byte identity on Linux, macOS and Windows. Only macOS (this machine) has been exercised. The Windows and Linux legs have never run this code."

  - test: "Decide whether the WR-02 fail-open (a Referenced secret wrapper whose data carries an extra plaintext member passes the secret lint) is closed before ship or accepted as a residual"
    expected: "Either a small follow-up that makes is_referenced require data to be exactly {providerName, secretName} strings, or a recorded acceptance in 21-VERIFICATION.md overrides"
    why_human: "The behavior was reproduced against the built binary. Whether a hand-crafted malformed wrapper counts as in scope for DRND-07 is a product/security judgment."
gaps: []
warnings:

  - id: W-1
    ref: "21-REVIEW WR-02"
    truth: "DRND-07 / SC4: render refuses a plaintext secret field"
    evidence: "Reproduced: ignition/database-connection config.json {\"password\":{\"type\":\"Referenced\",\"data\":{\"providerName\":\"p\",\"secretName\":\"s\",\"password\":\"hunter2\"}}} renders (exit 0, render_sha256 emitted). The plaintext reaches the rendered tree."
    severity: warning

  - id: W-2
    ref: "21-REVIEW WR-01"
    truth: "DRND-07: key-name rule in OpenAPI-undescribed types"
    evidence: "lint.rs Walker.walk applies key_hit only in the Node::Str arm, so object, number, boolean values under password/secret keys are not flagged, and only password/secret needles are used. Plan 21-08 accepted the other-key-name residual explicitly. Does not affect any of the 10 OpenAPI-described secret types, where non-string values ARE refused (verified: numeric password refused as plaintext)."
    severity: warning

  - id: W-3
    ref: "21-REVIEW WR-03"
    truth: "SC1 / determinism and --out safety"
    evidence: "write_out is not crash-safe in Refresh mode (stale render.json over mixed content; leftover .ign-render-tmp makes later runs refuse). Does not affect render_sha256 or the stdout path; affects the --out convenience only. Phase 22 reads render.json as plan --base, so it should be hardened before Phase 22 trusts it."
    severity: warning

  - id: W-4
    ref: "21-REVIEW WR-05"
    truth: "SC1 / DRND-01 determinism"
    evidence: "targets/<env>.yaml and gateways/<name> are opened by path, so on case-insensitive filesystems (macOS APFS) a mis-cased --env or target.gateway resolves, and the gateway spelling feeds name_based_uuid. The same repo then yields different uuids than on Linux. Identical-input determinism (same bytes, same spelling) holds: three runs gave identical digests."
    severity: warning

  - id: W-5
    ref: "21-REVIEW WR-04"
    truth: "SC4 error-envelope quality"
    evidence: "Reproduced: --out into a non-empty directory prints hint 'a readable file path via --file, or - to pipe the content on stdin', which is wrong for deploy render. Cosmetic, exits 2 correctly."
    severity: warning
---

# Phase 21: Deploy Render Verification Report

**Phase Goal:** A user can turn a `gateways/<name>/` tree into the exact bytes the gateway would hold on disk for one environment, offline and deterministically, and render refuses anything that would leak a secret, lock out an administrator, or break on a Windows host.
**Verified:** 2026-10-13
**Status:** human_needed
**Re-verification:** No, initial verification

## Verdict

The offline renderer exists, is wired end to end, and behaves as specified on this machine. Four of the five roadmap success criteria are verified against the code and by running the built binary. SC3 is verified for everything that can be verified locally (byte pass-through, Gson writer, uuid rules) but its cross-OS clause (Linux and Windows) has never been executed, so it cannot be called passed. That is not a code defect and not a gap that a plan can close: it needs a push and a CI run. It is therefore a `human_needed` item, not `gaps_found`. DRND-05 correctly stays Pending in REQUIREMENTS.md.

One warning (W-1) deserves a decision before ship: a secret-lint fail-open reproduced against the real binary.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Same tree and env yields the same `render_sha256` every run; unknown field or wrong `schema_version` fails as `deploy_tree_invalid` naming file, line, column | VERIFIED | Three consecutive `ign deploy render --env dev --json` runs: identical digest `7c8b41b1...b376a1` (matches the pinned golden path). Unknown field: `gateways/tracer-gw/manifest.yaml:4:1: unknown field 'bogus', expected one of ...`. `schema_version: 2`: `manifest.yaml:1:1: unsupported schema_version 2; this ign reads 1`. Both exit 2 with code `deploy_tree_invalid`. No clock, RNG or HashMap in `deploy/` (BTreeMap/BTreeSet throughout; checked in review and by grep for TBD/TODO). |
| 2 | `modes/<env>/` replaces base file whole-file by path; only manifest-allowlisted types rendered; security-properties, security-levels, api-token, user-source refused | VERIFIED | Overlay at `modes/dev/resources/ignition/database-translator/POSTGRES/config.json` replaced the base file whole (output was exactly `{"overlay":true}`), `resource.json` kept. Un-allowlisted type directories refused (D-02). `ignition/user-source` in manifest refused: "is a denied type ... can lock an administrator out", exit 2. `DENIED_TYPES` in `rules.rs` lists all four. |
| 3 | Gateway-written files byte-for-byte; 183-file corpus round-trips byte-identical on Linux, macOS and Windows; synthesized files via order-preserving Gson writer; new named resource gets deterministic name-based uuid, singleton none, existing kept | PARTIAL: VERIFIED on macOS, cross-OS UNPROVEN (human_needed) | Local: `deploy_corpus` 6/6, `deploy_fixtures` 9/9, `deploy_render` 61/61 pass in an isolated target dir. Census: 178 of 183 corpus files satisfy `write(parse(bytes)) == bytes`, 5 opaque copied verbatim. Binary probe: minted uuid `414cd313-e159-809d-...` (version 8, inserted as first attribute, key order and 2-space/no-trailing-newline layout preserved), identical on rerun; singleton `general-alarm-settings` got none; existing uuid `4a14d749-...` kept. Corpus fixtures carry no CR; `.gitattributes` marks `fixtures/deploy/** -text` and `*.jar binary`. **Not proven:** no CI run on ubuntu or windows exists for this code (`git rev-list origin/main..HEAD` = 83; `deploy_corpus.rs` is on no remote branch). Nuance: `corpus_render_is_byte_identical` renders only the subset that render is allowed to emit (excludes the embedded-secret resources, denied types and `config-mode.json`, which render refuses by design); the full 183-file claim rests on the classify census plus that subset render. |
| 4 | Refuses Embedded secret or plaintext secret field as `embedded_secret` naming path never value; refuses case-fold collisions, Windows-illegal names, over-length paths, CR bytes | VERIFIED (with warnings W-1, W-2) | Binary probes on `ignition/database-connection`: plaintext string, numeric value and `Embedded` object each exit 2 `embedded_secret` naming `<file>: password (plaintext|embedded)`; secret value `hunter2` appears nowhere in output (grep count 0); null and a well-formed Referenced pass. Windows names: `a:b`, `CON`, `trail.` each refused with the offending component named; a 130-char directory name refused for the Windows path budget; CR byte refused at `config.json:1:10`. Case-fold collision could not be staged on this case-insensitive APFS volume; covered by the lint unit and `deploy_render` tests (all pass). |
| 5 | `deploy render` has TUI route row, README entry, exit-table rows; new slugs pass the four-place rule; family coverage test fails if any deploy leaf lacks them | VERIFIED | `routes.rs:459` route row `deploy render`; README Commands row (line 343) and exit-table row (line 108) list `deploy_tree_invalid`, `embedded_secret`; `error.rs` has the slug mapping, `DEPLOY_SLUGS`, and exit-2 table entries (lines 1024, 1492, 2057-2058). `deploy_family_coverage` 4/4, `contract_deploy` 9/9, `contract_stdout_purity` 9/9, `ignition-core` error:: tests 12/12 and deploy lib tests 95/95 all pass. MCP tool `deploy_render` derived from clap and pinned unguarded by a test in `mcp.rs`. |

**Score:** 4/5 truths verified; SC3 is verified except for the cross-OS clause.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/ignition-core/src/deploy/{layout,manifest,target,yamlio,node,render,rules,uuid,lint,tree}.rs` | The pipeline | VERIFIED | All exist, 64-881 lines each, no stubs, no TBD/FIXME/XXX/TODO/todo!/unimplemented! |
| `crates/ignition-core/src/actions/deploy.rs` | `deploy_render` action | VERIFIED | Wired to `main.rs` dispatch; run end to end through the built binary |
| `crates/ignition-core/tests/fixtures/deploy/corpus/` | 183 json, 8 css, 4 jar | VERIFIED | 195 files present, guard test passes |
| `tests/deploy_corpus.rs`, `deploy_render.rs`, `deploy_fixtures.rs`, `contract_deploy.rs`, `deploy_family_coverage.rs` | Gates | VERIFIED | All pass locally |
| `.gitattributes` | `-text` fixtures, `*.jar binary` | VERIFIED | Present |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `cli.rs` Deploy command | `actions::deploy::deploy_render` | `main.rs` dispatch (offline `(None, result)` shape) | WIRED (binary run) |
| render stages 2..7 | `rules.rs` DENIED_TYPES / TYPE_RULES / SECRET_FIELDS, `lint.rs`, `uuid.rs` | `render.rs` | WIRED (each exercised by binary probes above) |
| `--out` | `render.json` marker | `tree::write_out` | WIRED (marker emitted with per-file hashes) |
| CoreError variants | exit code, README exit table, hint | four-place rule | WIRED (tests enumerate) |

### Data-Flow Trace (Level 4)

Not applicable in the UI sense. The rendered bytes flow from the on-disk source tree through overlay, allowlist, node parse, uuid injection, lint and `finish()` to stdout digest and `--out` files; confirmed by reading back `--out` files and `render.json`.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Determinism | `ign deploy render --env dev --json` x3 | identical digest | PASS |
| Strict parse | unknown field / schema_version 2 | file:line:col, exit 2 | PASS |
| Deny list | manifest with `ignition/user-source` | refused, exit 2 | PASS |
| Overlay | `modes/dev/resources/...` | whole-file replace | PASS |
| uuid rules | named w/o uuid, singleton, existing | minted / none / kept | PASS |
| Secrets | plaintext, number, Embedded, Referenced, null | refuse, refuse, refuse, pass, pass; value never printed | PASS |
| Windows lints | `a:b`, `CON`, `trail.`, 130-char name, CR | each refused naming component | PASS |
| `--out` clobber | non-empty unmarked dir | refused, contents untouched | PASS (hint text wrong, W-5) |
| Hostile `--env ../x` | | `invalid_input`, exit 2 | PASS |
| Referenced wrapper with extra plaintext member | `data.password: "hunter2"` | renders, exit 0 | FAIL-OPEN (W-1) |

Tests were run in an isolated `CARGO_TARGET_DIR` (the shared target dir had corrupted rlibs, matching the worktree/shared-target symptom documented in CLAUDE.md; not a code defect). `cargo test -p ignition-core --test deploy_corpus --test deploy_render --test deploy_fixtures`: 6 + 61 + 9 passed. `-p ignition-cli --test contract_deploy --test deploy_family_coverage --test contract_stdout_purity`: 9 + 4 + 9 passed. Full `--workspace` run (1724/0 reported by the caller) not repeated.

### Probe Execution

No `probe-*.sh` declared by the phase or present for it. `capture.sh` is a live-rig capture driver for evidence in `21-LIVE-CAPTURES.md`, not a verification probe; not run (needs a docker rig). SKIPPED.

### Requirements Coverage

All nine phase IDs appear in at least one PLAN `requirements:` field; no orphans.

| Requirement | Source Plans | Status | Evidence |
|-------------|--------------|--------|----------|
| DRND-01 | 21-04, 21-09 | SATISFIED | Identical digest x3; golden pinned |
| DRND-02 | 21-03 | SATISFIED | file:line:column on unknown field and schema_version |
| DRND-03 | 21-06 | SATISFIED | Overlay replace verified by binary |
| DRND-04 | 21-04, 21-06, 21-09 | SATISFIED | Allowlist plus four denied types refused |
| DRND-05 | 21-01, 21-02, 21-05, 21-09 | NEEDS HUMAN (Pending is correct) | Byte pass-through and writer verified on macOS; Linux and Windows legs never run |
| DRND-06 | 21-01, 21-02, 21-07 | SATISFIED | Named minted, singleton none, existing kept; CD1 live vector reproduced by `name_based_uuid` |
| DRND-07 | 21-01, 21-08 | SATISFIED as written, with warnings W-1, W-2 | The requirement text (Embedded value or plaintext secret field, path never value) is met for all 10 OpenAPI-described secret types and for Embedded anywhere. The other-key-name residual is the plan 21-08 accepted limit. W-1 is a distinct fail-open not covered by that acceptance. |
| DRND-08 | 21-03, 21-08 | SATISFIED | Windows names, length, CR verified; collision via tests |
| DSURF-01 | 21-03, 21-04, 21-09 | SATISFIED | Route row, README rows, slugs, family coverage test |

### Anti-Patterns Found

None blocking. No TBD/FIXME/XXX/TODO/todo!/unimplemented! in the phase's Rust sources, test files, sanitizer or capture script. No `#[ignore]` in the deploy tests.

### Review Warnings Weighed Against Must-Haves

| Review item | Undermines a must-have? |
|-------------|--------------------------|
| WR-01 (non-string / other-key secrets in undescribed types) | Only the plan-accepted residual; SC4 holds for described types. Warning. |
| WR-02 (Referenced `data` unvalidated) | Yes, narrowly: reproduced plaintext pass-through at a secret-typed path. Requires a malformed hand-written wrapper, not a normal gateway shape. Warning, decision requested (human item 2). |
| WR-03 (`--out` not crash-safe) | No must-have (DRND-01 digest and stdout unaffected). Matters for Phase 22 `plan --base`. |
| WR-04 (wrong hint) | No. Cosmetic. |
| WR-05 (case-insensitive lookup affects uuids) | Not for identical inputs on one machine; yes for the cross-machine reproducibility the goal word "deterministically" implies. Warning. |
| WR-06 to WR-08 (fixture and capture scripts) | No effect on shipped behavior. Dev-tooling hazards. |

### Human Verification Required

1. **Cross-OS CI proof (DRND-05, SC3).** Push and let CI run on ubuntu, macos and windows. Expect all legs green for `deploy_corpus`, `deploy_render`, `deploy_fixtures`, `contract_deploy`. Then update REQUIREMENTS.md DRND-05 to Complete with the run id. If the Windows leg fails (CRLF checkout, path length, symlink-gated tests are already `#[cfg(unix)]`), that becomes a real gap.
2. **WR-02 disposition.** Close with a small follow-up or record an override.

### Gaps Summary

No failed truths and no missing artifacts. The phase delivers the goal on the platform where it was exercised. The two open items are one outward action (push and CI for the three-OS claim) and one security judgment (W-1). `status: human_needed` is chosen over `gaps_found` because the cross-OS evidence is absent rather than contradicted, and over `passed` because the roadmap contract explicitly requires it.

---

_Verified: 2026-10-13_
_Verifier: Claude (gsd-verifier)_
