---
phase: 25-guarded-project-deploy
reviewed: 2026-10-28T00:00:00Z
depth: standard
files_reviewed: 20
files_reviewed_list:
  - .github/actions/deploy-projects-apply/action.yml
  - .github/actions/deploy-projects-check/action.yml
  - .github/workflows/deploy-projects.yml
  - .github/workflows/release.yml
  - crates/ignition-cli/src/cli.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/tests/contract_adopt.rs
  - crates/ignition-core/src/actions/adopt.rs
  - crates/ignition-core/src/client/adopt.rs
  - crates/ignition-core/src/deploy/apply/run_store.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/owner_acl.rs
  - crates/ignition-core/src/deploy/project_artifact.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/tests/fixtures/live-adopt-rig/compose.yml
  - crates/ignition-core/tests/live_adopt_write_admission.rs
  - crates/ignition-core/tests/live_rig_module_injection.rs
  - docs/installation.md
  - README.md
  - RELEASING.md
findings:
  critical: 0
  warning: 5
  info: 6
  total: 11
status: issues_found
---

# Phase 25: Code Review Report (gap-closure pass, plans 25-22 to 25-27)

**Reviewed:** 2026-10-28
**Depth:** standard
**Files Reviewed:** 20
**Status:** issues_found

## Summary

Scope was the diff `7c7269e^..HEAD` for the listed files (README: changed hunks only). The gap-closure work is largely sound. No blockers were found:

- The SID-based ACL code (`owner_acl.rs`) validates the SID before it reaches `icacls`, fails closed, and has no fallback to a name or to an unrestricted file.
- The write-scope model in `client/adopt.rs` and `actions/adopt.rs` never derives a bare `Authenticated` write entry, and refuses a bare `Authenticated` or `Public` write level before any login.
- The composite actions validate every caller input before it reaches `GITHUB_ENV` or a shell. All inputs go through `env:`, never inline `${{ }}` in `run:`. The checkout, handoff and SHA-pin logic is consistent between the check and apply actions.
- The static-CRT release step checks the right thing.

The weaknesses are in adopt's failure handling after a key has been minted, one documented remediation command that cannot work on the gateways it targets, and a few CI-hardening gaps in the new actions.

## Structural Findings (fallow)

None provided for this pass.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: Write probe can fail after the key is minted, losing the only copy of the plaintext

**File:** `crates/ignition-core/src/actions/adopt.rs:490-527`
**Issue:** The new write half of step 4 runs `scan_projects()` immediately after the permissions PUT, before step 5 persists the token. On a write-probe failure the function returns `Err` and the freshly minted plaintext is dropped. The `(false, 403)` arm, the `(true, Ok)` arm and the `(_, Err)` arm all do this.

- The key now exists on the gateway, and its plaintext is unrecoverable by construction.
- A re-run takes the found-key path. It fixes permissions, then has no probe credential (`compose_credential` is `None`) and returns `SecretUnavailable`. That is the same "minted-then-failed" trap the earlier review round closed for the read probe.
- The user has to delete the key in the gateway UI by hand.
- The new live test's own comments show the failure is realistic. The test sleeps 3 s after every permissions PUT ("Let the gateway apply the new permission lists before probing", lines 613, 924 and 981). `adopt` probes the write with no wait and no retry, so a stale 403 right after the PUT produces a hard failure plus an orphan key.

**Fix:** Retry the write probe on 403 a few times with a short backoff, when the permissions step just wrote (`changed == true`). When the write probe still fails after a key was minted, persist the token first, then return the error. That way the re-run has a credential and repairs in place. For the `--read-only` `(true, Ok)` case, say that the minted key is unusable and unrecoverable. For example:

```rust
// before matching on write_probe
let mut write_probe = probe.api().scan_projects().await;
if changed && !opts.read_only {
    for _ in 0..4 {
        if !matches!(write_probe, Err(CoreError::Auth { status: 403, .. })) { break; }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        write_probe = probe.api().scan_projects().await;
    }
}
```

### WR-02: The documented remediation for pre-release gateways is refused by the code it points at

**File:** `README.md` (new "A read-only check token" section, `ign adopt --profile dev --restrict-write`); `crates/ignition-core/src/actions/adopt.rs:326-342`
**Issue:** The README tells operators of gateways adopted by earlier releases to run `ign adopt --profile dev --restrict-write` to close the bare-`Authenticated` write hole. On such a gateway the existing `ign-cli` key holds only the Administrator level. The found-key branch now requires the key to hold every requested level, including `Authenticated/IgnCliWrite`, and returns `API key ... exists at level ... was requested` before the permissions step runs. The command fails exactly where it is recommended. The next README subsection ("Gateways adopted by earlier releases") describes that refusal, so the two sections contradict each other.

Even a correctly ordered sequence (delete the key, then `--restrict-write`) works only by accident. The key step mints the new key first, then the permissions step removes the bare entry.

**Fix:** Either (a) reorder the README so the key deletion comes first and the `--restrict-write` command mints and restricts in one run, or (b) in code, turn the level mismatch into a `Warn` step row when `opts.restrict_write` is set, so the permissions work still runs, and let the probe report the result. Option (b) also keeps `--restrict-write` usable as an operator repair tool.

### WR-03: Artifact name collides when one workflow run applies more than once to the same environment

**File:** `.github/actions/deploy-projects-apply/action.yml:481-487` and `:498`
**Issue:** The upload name is `ign-deploy-projects-${{ inputs.env }}-${{ github.run_id }}-${{ github.run_attempt }}`. `actions/upload-artifact@v4` and later reject a second artifact with the same name in a run, with a 409 conflict. A caller that deploys two projects, or two `repo-dir`s, to the same environment from one run (a matrix or sequential jobs) loses the second upload. Because the step has `if: always()`, the failure lands after the deploy, so a successful deploy shows a failed job. The reusable workflow's `concurrency` group suggests sequential same-env use in one repo is expected.

**Fix:** Add a caller-controllable discriminator. For example, add an optional `artifact-suffix` input, or include `github.job` and the sanitized `repo-dir` in the name. Pass the same name to the report step through `IGN_ARTIFACT`.

### WR-04: The installed `ign` binary is verified only against a checksum fetched from the same place

**File:** `.github/actions/deploy-projects-apply/action.yml:182-250` (identical block in the check action)
**Issue:** The install step downloads `ign-<tag>-<target>.<ext>` and its `.sha256` from the same release URL on `TheThoughtagen/ignition-cli`. A tampered or replaced release asset ships with a matching checksum. The check passes and `ign` then runs holding the production write token and the deploy SSH key. This is a corruption check, not an authenticity check. The binary runs with the strongest secrets in the pipeline, and it comes from a different GitHub owner than the one the targets repo uses (WhiskeyHouse).

**Fix:** Verify provenance. For example, publish GitHub artifact attestations in `release.yml` and run `gh attestation verify` in the action. Or let the caller pin the expected SHA-256 (an `ign-sha256` input, mandatory for the apply action). At minimum, document the trust assumption next to `IGN_RELEASE_BASE`.

### WR-05: The `--read-only` failure text can send the operator to delete their working admin key

**File:** `crates/ignition-core/src/actions/adopt.rs:576-583`
**Issue:** `--read-only` defaults `--key-name` to `ign-cli`, the same name as the working write key. If that key exists, the found-key check passes, because the requested levels are a subset of the key's levels. The probe then uses the profile's resolved credential. For the admin profile that credential writes, so the `(true, Ok(()))` arm fires. It tells the operator to "delete this key in the gateway and re-run". The key it names is the live deploy key. Nothing before this point tells the user that `--read-only` needs a distinct `--key-name`.

**Fix:** Refuse early with `InvalidInput` when `--read-only` is combined with the default key name, or when the found key holds more levels than requested. Make the final message name the key (`opts.key_name`) so the operator can see which one it is.

## Info

### IN-01: `NestedExact` is dead outside tests

**File:** `crates/ignition-core/src/actions/adopt.rs:269-286`, `:294-316`
**Issue:** `WRITE_ADMISSION` is a `const` fixed to `Dedicated`. The `NestedExact` variant and its `resolve_write_levels_for` arm are reachable only from unit tests. The measured result already rules that model out, so the variant is speculative code with a `pub` enum.
**Fix:** Remove the variant and its arm. Keep the recorded decision in the doc comment.

### IN-02: `AdoptOptions` grew three required `pub` fields

**File:** `crates/ignition-core/src/actions/adopt.rs:104-119`
**Issue:** `AdoptOptions` is a `pub` struct in a crate published to crates.io, and it has no `Default`. Adding `write_levels`, `read_only` and `restrict_write` breaks every external struct literal. In-repo callers (`main.rs`, `live_rig_module_injection.rs`) were updated, but a library consumer would break on a minor bump.
**Fix:** Derive or implement `Default` and mark the struct `#[non_exhaustive]`, or bump the version accordingly.

### IN-03: The live test re-implements a library helper

**File:** `crates/ignition-core/tests/live_adopt_write_admission.rs:484`
**Issue:** The test defines its own `is_open_entry` while importing `write_admits_every_key` from the library. The two can drift, and the capture would then disagree with the code under test.
**Fix:** Call `write_admits_every_key` (or export the library's helper) instead of keeping a second copy.

### IN-04: `ssh-key-env` uses a denylist

**File:** `.github/actions/deploy-projects-apply/action.yml:385-389` (and the check action)
**Issue:** The reserved-name denylist (`PATH`, `HOME`, `BASH_ENV`, `GITHUB_*`, `LD_*` and so on) misses variables that change how the child processes behave. Examples are `GIT_SSH_COMMAND`, `GIT_CONFIG_*`, `GIT_ASKPASS`, `SSH_ASKPASS`, `PROMPT_COMMAND`, `NODE_OPTIONS`. Because the caller supplies this value, it is not an outside-attacker path. It is a footgun, since the key text becomes the variable's value.
**Fix:** Prefer an allowlist pattern such as `^[A-Z][A-Z0-9_]*_KEY$|^DEPLOY_[A-Z0-9_]+$`, or add `GIT_*` and `SSH_*` to the denylist.

### IN-05: Dead `IGN_BACKUP_*` plumbing and a sloppy `bad` concatenation in the actions

**File:** `.github/actions/deploy-projects-apply/action.yml:258-259`, `:328`, `:331`, `:471-472` (same in the check action for the first three)
**Issue:** `IGN_BACKUP_PROFILE`, `IGN_BACKUP_URL` and `IGN_BACKUP_TOKEN` are hardcoded to `''`. The backup branches of the shared blocks are dead in these actions. In the stage step, `bad="$bad$(find ...)"` joins the two `find` results without a separator, because command substitution strips the trailing newline. This is cosmetic, since it only garbles the error listing, and the `-n` test is still correct.
**Fix:** Drop the unused backup branches if the "shared" block markers allow it, or document that they are kept for byte-identical sharing. Use `bad="$bad"$'\n'"$(find ...)"`.

### IN-06: Actions are pinned to major tags in jobs that hold production secrets

**File:** `.github/actions/deploy-projects-apply/action.yml:137,147,483,492` (and the check action, `release.yml`)
**Issue:** `actions/checkout@v7`, `actions/github-script@v9` and `actions/upload-artifact@v7` are mutable tags. Only the apply job sees the write token. These are first-party actions, so the risk is low, but a pin to a commit SHA is the usual hardening for a job with this much access.
**Fix:** Pin to full commit SHAs with a trailing version comment, and let Dependabot update them.

---

_Reviewed: 2026-10-28_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
