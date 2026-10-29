---
phase: 25-guarded-project-deploy
fixed_at: 2026-10-29T00:00:00Z
review_path: .planning/phases/25-guarded-project-deploy/25-REVIEW.md
iteration: 1
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 25: Code Review Fix Report

**Fixed at:** 2026-10-29
**Source review:** .planning/phases/25-guarded-project-deploy/25-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5 (WR-01 to WR-05; Info findings IN-01 to IN-06 were out of scope for this pass)
- Fixed: 5
- Skipped: 0

Verification ran in the main checkout (`workflow.use_worktrees=false`, no worktree), so the numbers below are reproducible from the tree as it stands.

## Fixed Issues

### WR-01: Write probe can fail after the key is minted, losing the only copy of the plaintext

**Files modified:** `crates/ignition-core/src/actions/adopt.rs`
**Commit:** 159d46a
**Status:** fixed: requires human verification (error-handling and ordering logic)
**Applied fix:**
- The write probe now retries a 403 up to 4 times, 2 s apart, but only when the permissions step just wrote and the run is not `--read-only` (`probe_write_with_retry`).
- When the probe still fails after this run minted a key, the token is persisted to the keyring and the profile is pointed at it before the error is returned (`keep_minted_token` and `persist_after_failed_probe`). A re-run then finds the key, has a credential, and repairs in place instead of dead-ending on `SecretUnavailable`. The error text says the key was saved, or, when no keyring is usable, names the key to delete. An `Internal` error gains this note; other error variants keep their type, so their exit codes do not change.
- A `--read-only` key that turns out to be able to write is deliberately NOT persisted. Its message now names `opts.key_name` and says a key minted by this run is not stored.
- The env-var fallback is not used on the failure path, because it would have to print the plaintext inside an error. A headless host therefore still gets the delete-and-rerun note, not a recovered key.
- Offline tests (in `actions::adopt::tests`): retry then success, bounded give-up (1 + 4 probes), no retry when nothing was written or for a non-403, which failures keep the token (`write_probe_verdict`), persist-before-error ordering with the plaintext never in the error text, and nothing persisted for a found or unkeepable key.
- Not covered offline: the full `adopt()` flow (it needs an OIDC gateway). The live suite (`live_adopt_write_admission.rs`) was not run.

### WR-02: The documented remediation for pre-release gateways is refused by the code it points at

**Files modified:** `README.md`
**Commit:** e3dd2b4
**Status:** fixed: requires human verification (documented procedure, not exercised against a gateway)
**Applied fix:** Option (a), the README reorder. The code is unchanged.
- The "A read-only check token" snippet and the "Gateways adopted by earlier releases" section now say: delete the old `ign-cli` key first, then run `ign adopt --profile dev --restrict-write` once. That single run mints the key with the write level, adds it to Gateway Write Permissions, removes the bare `Authenticated` entry, and probes the new key for read and write.
- Why not option (b): turning the level mismatch into a `Warn` row when `--restrict-write` is set lets the permissions step run. The old Administrator-only key does not hold the write level, so once the bare entry is removed the write probe gets a 403 and adopt fails. The command would exit non-zero after it had already changed the write list. The existing adopt contract tests and the non-`--restrict-write` refusal would have stayed intact, but the end state was worse than the documented order, so (a) was chosen.

### WR-03: Artifact name collides when one workflow run applies more than once to the same environment

**Files modified:** `.github/actions/deploy-projects-apply/action.yml`, `.github/workflows/deploy-projects.yml`, `README.md`, `.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh`
**Commit:** cf9e648
**Applied fix:**
- New optional `artifact-suffix` input (default empty, so existing artifact names are unchanged) on the apply action and on the reusable `deploy-projects.yml` workflow, which passes it through.
- A new step, `Name the deploy records artifact` (`body:artifact-name`), validates the suffix (empty, or 1 to 40 characters of letters, digits, `.`, `_`, `-`, starting with a letter or digit) and writes `ign-deploy-projects-<env>[-<suffix>]-<run id>-<attempt>` to a step output. The upload and the report both use that output. The step sits after the handoff check, so the handoff stays the first step. The report keeps the old name as a fallback if the step never ran.
- README: `artifact-suffix` row added to the apply-action and reusable-workflow input tables.
- Harness: the pinned apply-input list now includes `artifact-suffix`, and a new case 4i runs the extracted body (plain, suffixed, and rejected suffixes) and checks that the upload and report take the name from the step.
- A caller that applies twice to one env in one run must now pass a different suffix for each apply. The default does not do this for them.

### WR-04: The installed `ign` binary is verified only against a checksum fetched from the same place

**Files modified:** `.github/actions/deploy-projects-check/action.yml`, `.github/actions/deploy-projects-apply/action.yml`, `.github/workflows/deploy-plan.yml`, `.github/workflows/deploy-apply.yml`, `.github/workflows/deploy-projects.yml`, `.github/workflows/release.yml`, `RELEASING.md`, `README.md`, `.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh`, `.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/release-test.sh`, `.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/readme-test.sh`
**Commits:** 876fb62 (install gate), 42250d7 (release.yml attest job, deploy-projects.yml passthrough), bcdf27b (README), e92d430 (bind to this run's build output and the tag ref)
**Status:** fixed: requires human verification (the attestation path can be exercised live only against a release built after this change)
**Applied fix:**
- The owner decision: opt-in attestation, not a caller-pinned digest. Callers who set nothing see no change.
- The gate lives in `shared:install-ign`, after the sha256 check and before extraction, in all four copies (both actions, `deploy-plan.yml`, `deploy-apply.yml`). The mode is read as `${IGN_VERIFY_ATTESTATION-false}`: unset means false, which keeps `deploy-plan.yml` and `deploy-apply.yml` unchanged in behaviour. The call is `gh attestation verify <asset> --repo TheThoughtagen/ignition-cli --signer-workflow TheThoughtagen/ignition-cli/.github/workflows/release.yml --source-ref refs/tags/<ign-version> --deny-self-hosted-runners`.
- Strict validation: anything other than exactly `true` or `false` is refused, and `true` with no `gh` on PATH is refused, both before any download.
- `GH_TOKEN` is set from `github.token` only when the input is exactly `true`, and the body runs `unset GH_TOKEN` before extraction in every mode, so the downloaded binary never sees it.
- `release.yml` gets a dedicated `attest` job (`needs: build`, `if: startsWith(github.ref, 'refs/tags/v')`). It is separate from the build job so that `id-token: write` and `attestations: write` are not available to the build job's third-party actions. After the orchestrator's security review (F6, F7) it attests the archives this run's build job uploaded as `ign-archive-<target>` artifacts, never the release's assets, so a `workflow_dispatch` re-run cannot sign what someone else put on the release; only tag-ref runs attest.
- `deploy-projects.yml` gains a boolean `verify-attestation` input (default false) passed to both actions, and requests no new permission.
- README: a Binary trust note (sha256 is integrity against the release; attestation is provenance from `release.yml`, bound to the tag; releases up to `v1.6.0-rc.2` have none) and a `verify-attestation` row in the three input tables.
- Tests: harness cases 3h, 3i and 4j (the install body is byte-identical to the 408f65b body for false and unset; true calls gh once with the exact argv before extraction; the binary sees no token; a failing gh, a malformed value and a missing gh all fail closed), release-test 6 to 9, readme-test 7.
- Not covered: a live `gh attestation verify` against a real attestation (needs the next release).

### WR-05: The `--read-only` failure text can send the operator to delete their working admin key

**Files modified:** `crates/ignition-core/src/actions/adopt.rs`, `crates/ignition-cli/src/cli.rs`, `crates/ignition-cli/tests/contract_adopt.rs`, `README.md`
**Commits:** 6786e9a, 71f11b9 (rustfmt of the new contract test)
**Status:** fixed: requires human verification (validation logic)
**Applied fix:**
- `adopt()` now refuses `--read-only` with the default key name (`ign-cli`) as `invalid_input` (exit 2) at the pre-login input stage. The message names the key via `opts.key_name`, names `--key-name`, and suggests `--key-name ign-ci-check`.
- The `(true, Ok)` probe message from WR-01 also names `opts.key_name`.
- The refusal is in a separate pure function (`check_read_only_key_name`), so the existing `resolve_write_levels` tests are unchanged.
- Tests: the unit test `read_only_refuses_the_default_key_name`, and two CLI contract tests, `read_only_with_the_default_key_name_is_refused_before_login` (exit 2, `invalid_input`, names `--key-name` and `ign-cli`, under 10 s against a dead port) and `read_only_with_its_own_key_name_is_not_refused_early`.
- The `--read-only` flag help and the README bullet now say it needs its own `--key-name`.
- Not implemented: the review's alternative of also refusing when a found key holds more levels than requested. The default-name refusal covers the reported path.

## Skipped Issues

None.

## Verification

Run in the main checkout after all commits:

| Check | Result |
|-------|--------|
| `cargo test -p ignition-core --lib adopt` | 23 passed, 0 failed |
| `cargo test -p ignition-cli --test contract_adopt` | 6 passed, 0 failed |
| `cargo test -p ignition-core --lib -- readme_exit_table_agreement exit_code_mapping_enumerated` | 2 passed, 0 failed |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `projects-test.sh` | 43 PASS, 0 FAIL, exit 0 |
| `readme-test.sh` | 9 PASS, 0 FAIL, exit 0 |
| `projects-test.sh` (after WR-04) | 46 PASS, 0 FAIL, exit 0 |
| `apply-test.sh` | 30 PASS, 0 FAIL, exit 0 |
| `release-test.sh` | 9 PASS, 0 FAIL, exit 0 |
| `readme-test.sh` (after WR-04) | 11 PASS, 0 FAIL, exit 0 |
| `plan-test.sh` | 17 PASS, 0 FAIL, exit 0 (docker available; case 2a ran the changed install body against the real v1.5.0 release) |
| `actionlint` on `release.yml`, `deploy-plan.yml`, `deploy-apply.yml` | clean |
| `actionlint` on `deploy-projects.yml` | clean once the two `job.workflow_repository` and `job.workflow_sha` "not defined" messages are ignored (as harness case 1a does; the installed actionlint predates them and reports the same 10 messages on the 408f65b file) |

The live adopt tests (`live_adopt_write_admission.rs`, `live_rig_module_injection.rs`) need a gateway and were not run.

---

_Fixed: 2026-10-29_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
