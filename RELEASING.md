# Releasing

One tag push ships everything: prebuilt binaries (5 targets + sha256 checksums), build-provenance attestations, the GitHub release, and the three crates to crates.io. No version-bump commit or PR is needed.

## The release flow

```bash
# 1. Tag a commit on main (annotated, vX.Y.Z or vX.Y.Z-pre). The tag IS the version.
git fetch origin
git tag -a v1.7.0 -m "v1.7.0 — <one-line summary>" origin/main

# 2. Push the tag — it triggers the whole pipeline
git push origin v1.7.0
```

Do not create the release from the GitHub UI or with `gh release create`: a tag made that way did not start the pipeline (v1.6.0, 2026-10-29), and the empty release had to be deleted. The pipeline creates the release itself.

The version in `Cargo.toml` on `main` is not read at release time. The pipeline stamps the tag's version into `Cargo.toml` (workspace version and the two path-dependency versions) in each job's checkout before it compiles or publishes. Bump `main`'s `Cargo.toml` only when you want local and `cargo install --git` builds to report a newer version.

The `.github/workflows/release.yml` pipeline then:

0. **version** — derives the version from the tag (`v1.7.0` → `1.7.0`). It refuses a tag that is not `vX.Y.Z` or `vX.Y.Z-pre` (so a milestone tag like `v1.6` releases nothing) and a tag whose commit is not on `main`.

1. **build** — release matrix on five targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`). Each Unix target uploads `ign-vX.Y-<target>.tar.gz` + `.sha256`; the Windows target uploads `ign-vX.Y-x86_64-pc-windows-msvc.zip` + `.zip.sha256`. Assets go to the GitHub release (created if it doesn't exist).
2. **attest** — after `build`, for tag-ref runs only (`if: startsWith(github.ref, 'refs/tags/v')`; a `workflow_dispatch` run from a branch attests nothing): downloads the archives this run's `build` job packaged (uploaded as the `ign-archive-<target>` artifacts, never read back from the release, so a re-run cannot sign assets someone else put on the release) and publishes a GitHub build-provenance attestation for each. Verify with `gh attestation verify <asset> --repo TheThoughtagen/ignition-cli --source-ref refs/tags/<tag>`; the deploy actions' `verify-attestation` input binds to that tag and refuses self-hosted-runner attestations. `id-token: write` and `attestations: write` live on this job only, which runs no third-party action. Only releases built after this change carry attestations; `v1.6.0-rc.2` and earlier do not.
3. **publish-crates** — after `build` proves the tag compiles everywhere: `cargo publish --workspace --allow-dirty` (ignition-core → ignition-tui → ignition-cli, ordered by cargo). Idempotent: if `ignition-cli@<version>` is already on crates.io the job skips cleanly, so tag re-pushes and re-runs don't fail.

Watch it: `gh run watch` (or the Actions tab). When it's green, update the release notes with a `cargo install ignition-cli` line if you want it front and center.

## The Windows binary

The Windows build links the C runtime statically (`-C target-feature=+crt-static`, passed through setup-rust-toolchain's `rustflags` input for the `x86_64-pc-windows-msvc` matrix entry only), so `ign.exe` runs on a Windows Server without the VC++ redistributable. Without it, `ign.exe` imports `vcruntime140.dll` and exits `0xC0000135` (STATUS_DLL_NOT_FOUND) on a clean machine.

The flag is not in a `.cargo/config.toml` because setup-rust-toolchain exports `RUSTFLAGS`, and `RUSTFLAGS` overrides config-file rustflags, so a config-file flag would be dropped in CI without an error.

Before it packages the zip, the release job runs `dumpbin /dependents` on `ign.exe`. It fails when the import list has no `KERNEL32.dll` (the list was not read) or names a `vcruntime` or `msvcp` DLL, so such a binary is never uploaded. The list is in the job log and the job summary.

To check a downloaded asset by hand:

```bash
# Windows, from a Developer PowerShell
dumpbin /dependents ign.exe
# macOS or Linux (LLVM objdump, the default on macOS)
objdump -p ign.exe | grep 'DLL Name'
```

The list must contain `KERNEL32.dll` and no `vcruntime*.dll` or `msvcp*.dll`.

## One-time setup (already done for this repo)

- `CARGO_REGISTRY_TOKEN` repo secret — a crates.io token from
  <https://crates.io/settings/tokens> with **publish-new** and
  **publish-update** scopes (new covers each crate's first version; update is
  required for every later version of the same crate).
  Set with: `gh secret set CARGO_REGISTRY_TOKEN`
- The three crate names (`ignition-cli`, `ignition-core`, `ignition-tui`) are
  published under this token's account. (`ign` was already taken on crates.io;
  the binary is still `ign`.)
- The WebDev route sources live inside `crates/ignition-core/webdev/` —
  required so `include_str!` files ship inside the package. Don't move them
  back to the repo root.

## Rules

- **All three crates version in lockstep** (single `workspace.package`
  version). crates.io won't republish an existing version, so every release —
  even a one-crate fix — bumps all three together. That's deliberate.
- **Tags are the release.** Don't publish from a branch; the pipeline builds
  exactly what the tag points at.
- **Never move a published tag** once people may have installed from it;
  cut a new patch version instead.
- **MSRV 1.88** — CI and release builds use stable; keep new dependencies at
  or above that floor.
