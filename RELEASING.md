# Releasing

One push ships everything: prebuilt binaries (4 targets + sha256 checksums), the GitHub release, and the three crates to crates.io.

## The release flow

```bash
# 1. Bump the version — single-sourced in the root Cargo.toml
$EDITOR Cargo.toml          # [workspace.package] version = "1.1.0"

# 2. Commit and tag (annotated, vX.Y)
git commit -am "chore: release v1.1.0"
git tag -a v1.1.0 -m "v1.1.0 — <one-line summary>"

# 3. Push both — the tag triggers the whole pipeline
git push origin main v1.1.0
```

The `.github/workflows/release.yml` pipeline then:

1. **build** — release matrix on four targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`), each uploading `ign-vX.Y-<target>.tar.gz` + `.sha256` to the GitHub release (created if it doesn't exist).
2. **publish-crates** — after `build` proves the tag compiles everywhere: `cargo publish --workspace` (ignition-core → ignition-tui → ignition-cli, ordered by cargo). Idempotent: if `ignition-cli@<version>` is already on crates.io the job skips cleanly, so tag re-pushes and re-runs don't fail.

Watch it: `gh run watch` (or the Actions tab). When it's green, update the release notes with a `cargo install ignition-cli` line if you want it front and center.

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
