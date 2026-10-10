# Technology Stack

**Project:** ignition-cli (`ign`) - milestone v1.6 "Declared Gateway Deploy" (`ign deploy {render,plan,apply,rollback,verify,projects}`)
**Researched:** 2026-10-10
**Scope:** ONLY what the new deploy surface adds. The existing stack (reqwest 0.13 + rustls/ring, serde_json, zip 8.6, sha2, base64, getrandom 0.2, tokio incl. `process`, quick-xml, csv, clap, ratatui, MCP/LSP transports) is validated and not re-researched.
**Overall confidence:** HIGH (every version/MSRV below was read from the crates.io API on 2026-10-10; MSRV-sensitive picks were compiled in a `rust:1.88` container; the Gson claims were tested against 183 real gateway-written JSON files).

## Verdict in one paragraph

v1.6 needs **one new crate** (`yaml_serde`, +2 lockfile entries) and **one dev-to-runtime promotion** (`similar`, already locked). Everything else is already in the workspace graph. SSH/SFTP is done by **shelling out to the system `ssh` and `sftp` via `tokio::process`** behind a `RemoteShell` trait seam (same pattern as `ComposeRunner`), not by a Rust SSH library. The Gson-compatible writer is a ~100-line local module on top of `serde_json`'s `PrettyFormatter` (proven byte-identical on 178 of 183 real files; the other 5 are written by different writers, so the render must be passthrough-by-default). UUID v4 is 8 lines over the `getrandom 0.2` already declared. **Do not enable `serde_json/preserve_order` or `arbitrary_precision` workspace-wide** (feature unification would change key order of every `json!` payload in a frozen-contract CLI).

## Recommended Stack

### New dependencies

| Technology | Version | Purpose | Why | MSRV |
|------------|---------|---------|-----|------|
| `yaml_serde` | `0.10` (0.10.7, 2026-08-18) | Parse `manifest.yaml`, `targets/<env>.yaml`, `gateway-deps.yaml` (read-only; `render` emits JSON, never YAML) | The official continuation of dtolnay's `serde_yaml`, published by the YAML organization (github.com/yaml/yaml-serde). 6 releases in 2026. API-compatible, so `#[serde(deny_unknown_fields)]` and `from_str` work as people already know. Backend `libyaml-rs` is c2rust-transpiled Rust: no C toolchain, so the Windows-MSVC, aarch64-linux and future musl release targets are unaffected (the same reason the repo picked rustls+ring over aws-lc). Compiled and run on rustc 1.88: `deny_unknown_fields` rejects typos with a field list and `at line L column C`; `flag: no` is a type error rather than silently `false` (no YAML 1.1 "Norway problem"). | 1.82 (verified building on 1.88) |

### Promoted from dev-dependency (zero new lockfile entries)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `similar` | `3` (locked 3.2.0) with `default-features = false, features = ["text"]` | Unified line diff of the pretty-printed old/new content of *changed* resources in the `plan` preview | Already compiled today as snapbox's diff engine; promoting it to `ignition-core` `[dependencies]` adds a production edge but no new crate. MSRV 1.85. |

### Reused as-is (no change, new consumers only)

| Existing dep | New consumer in v1.6 |
|--------------|----------------------|
| `tokio` (`process`, `time`, `io-util`) | `ssh`/`sftp` child processes, per-call timeouts, `kill_on_drop` |
| `base64` 0.22 | `powershell -EncodedCommand` payload (UTF-16LE then base64) |
| `sha2` 0.10 | Per-file and archive sha256 for hash-verify, drift, project-zip pin check |
| `zip` 8.6 (`deflate`) | Build the deterministic render archive that is uploaded (fixed timestamps, sorted entries, forward-slash names) |
| `serde_json` 1.0.151 | Gson writer (custom `Formatter`), semantic diff over `Value`, API fixtures |
| `getrandom` 0.2 | `attributes.uuid` v4 generation |
| `tempfile` | Batch file for `sftp -b`, key file materialized from CI secret (mode 0600) |
| `std::fs::read_dir` | Tree walking (existing `workspace.rs` already does this; `walkdir` is not in the normal graph, do not add it) |
| `proptest` (dev) | Writer idempotence / round-trip properties |

### Core framework / database / infrastructure

No change. Deploy runs on the existing runtime and talks to the gateway through the existing reqwest client; the Config API surface (scan-lock, `scan/config`, mode, resource PUT with `signature`) is client code in `crates/ignition-core/src/client/<family>.rs`, not a dependency question.

## 1. SSH/SFTP transport: shell out to system OpenSSH

**Recommendation: `tokio::process::Command` driving the system `ssh` and `sftp`, wrapped in a `RemoteShell` trait.** No SSH crate.

| Criterion | system `ssh`/`sftp` (recommended) | `russh` + `russh-sftp` | `ssh2` (libssh2) | `openssh` crate |
|-----------|-----------------------------------|------------------------|------------------|-----------------|
| Latest | OpenSSH 10.3 (this Mac) | russh 0.64.1, russh-sftp 3.0.1 | 0.9.6 (2026-06-30) | 0.11.6 (2025-12-03) |
| MSRV vs our 1.88 | n/a | **0.64.x and 0.63.2/3 need 1.89**; 0.63.1 (1.85) is the newest that fits | not declared; needs C build | 1.63 |
| New crates | 0 | **+124 crate versions** (178-crate tree; ours is ~331). Includes RustCrypto pre-releases (`ssh-key 0.7.0-rc.11`, digest 0.11, rand 0.10) | libssh2-sys + openssl-sys (C) | few, but see platform |
| Windows client | Win32-OpenSSH ships with Windows; ssh.exe/sftp.exe | works, but only **Pageant** agent (not the OpenSSH named pipe) | libssh2's WinCNG backend lacks ed25519 (MEDIUM, web-sourced); the fix is an OpenSSL build | **Unix only** ("only works on unix"; relies on ControlMaster mux sockets) |
| Cross-compile in CI | nothing to build | ok with `ring` feature | needs target OpenSSL/cmake sysroot for aarch64-linux; the repo deliberately kept C toolchains out | n/a |
| Key/agent handling | full: `~/.ssh/config`, `IdentityAgent`, 1Password/Bitwarden agents, `ProxyJump`, `known_hosts` | reimplement: no `ssh_config`, no `IdentityAgent`, no passphrase prompt | reimplement | delegates to ssh |

**Why shell-out wins (the deciding factor is this user's real environment, not benchmarks):**

1. `~/.ssh/config` has per-host blocks for `WIGNVM01`/`WIGNVM04` plus a `Host *` 1Password `IdentityAgent` (and a Bitwarden agent for other hosts). A pure-Rust client ignores all of it; the CLI would need its own key discovery, agent-socket selection, passphrase and host-key UX, none of which is deploy logic. System `ssh` already does it identically on macOS, Linux and Windows.
2. MSRV: the newest russh (0.64.x) already requires 1.89. Adopting it forces an MSRV bump or a pin to a 5-version-old line.
3. "Lean dependency tree" is an explicit project constraint (Constraints in PROJECT.md). +124 crates including pre-release crypto, in the tool that holds the production deploy key, is the wrong trade for a feature that needs: one file upload, one command execution.
4. The process-spawn pattern already exists and is tested (`ComposeRunner`, `edit.rs`): one trait, a real impl, test doubles. `RemoteShell` follows it exactly.
5. `openssh` is out regardless: Unix-only, and WFLWVM01 (Windows runner) is a supported client.

**Seam (put in `crates/ignition-core/src/deploy/remote.rs`, mirrors `rig/compose.rs`):**

```rust
#[async_trait]
pub trait RemoteShell: Send + Sync {
    async fn run_ps(&self, script: &str, timeout: Duration) -> Result<RemoteOutput, CoreError>;
    async fn put(&self, local: &Path, remote: &str, timeout: Duration) -> Result<(), CoreError>;
    async fn get(&self, remote: &str, local: &Path, timeout: Duration) -> Result<(), CoreError>;
}
```

The production impl holds `SshProgram { ssh: PathBuf, sftp: PathBuf }` (injectable so a process-level test can point it at a tiny fake binary built from the test crate on all three OSes). Test doubles record scripts and return canned `RemoteOutput`.

**Invocation (argv, never a shell string):**

```
ssh -T -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3
    -o StrictHostKeyChecking=yes [-i <keyfile> -o IdentitiesOnly=yes] <user@host>
    powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand <b64>
sftp -b <batchfile> -o BatchMode=yes -o ConnectTimeout=10 [-i ...] <user@host>
```

`-i`/`IdentitiesOnly` only when `targets/<env>.yaml` names a key (CI: write the secret to a 0600 temp file); otherwise leave key selection to the user's ssh config. `StrictHostKeyChecking=yes` (never `no`): host-key pinning is a security property of a prod deploy tool; CI pre-seeds `known_hosts`.

**`-EncodedCommand` payload:** `base64(utf16le(script))` using `str::encode_utf16()` + `u16::to_le_bytes` + the existing `base64` crate; nothing to add.

**Windows-specific integration constraints (each one bites in practice; bake them in):**

- **Prelude every script** with `$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)` and `exit` explicitly. PowerShell otherwise serializes progress records as CLIXML on stderr when it has no console (LOW, web-sourced; cheap to guard). Emit one sentinel-prefixed JSON line (`IGN-RESULT:{...}`) and parse only that, so MOTD banners and stray output cannot corrupt the result.
- **Command-line length:** sshd hands the command to the default shell. cmd.exe caps at 8,191 chars, CreateProcess at 32,767. Base64(UTF-16LE) inflates 2.67x, so the script source must stay under about 3,000 chars to be safe on a cmd default shell. Assert a max encoded length at build time; for anything bigger, ship a `.ps1` inside the uploaded archive and call it from a one-line `-EncodedCommand`.
- **Windows PowerShell 5.1 only** (`powershell.exe`, present on every Windows Server). No `&&`, `?.`, ternary. For zip work use `[System.IO.Compression.ZipFile]` (.NET) rather than `Compress-Archive`/`Expand-Archive`, which have a long record of backslash/path bugs on 5.1 (MEDIUM). For the mirror-with-deletes step use `robocopy <staging> <target> /MIR` and treat exit codes 0-7 as success.
- **Win32-OpenSSH client stdin bug** (openssh-portable bug 2965): `ssh` with stdin redirected from NUL can hang on Windows. Spawn with `Stdio::piped()` and drop the stdin handle immediately (EOF) instead of `Stdio::null()`, on every OS. Also set `kill_on_drop(true)` and wrap every call in `tokio::time::timeout`.
- **No ControlMaster on Windows ssh.exe**, so every call pays a full handshake. Design for few round-trips: upload the archive and verify its hash BEFORE taking the scan-lock; inside the lock window run ONE remote script (extract to staging, mirror, report) so `holdTimeout` covers a single ssh call plus the scan POST. This directly serves spec §6 step 4.3 ("measured on the real transport").
- **SFTP path syntax on Win32-OpenSSH** (`/C:/...` vs `C:/...`) differs from Unix; capture it live on WIGNVM01 first (project capture-first rule). Upload to a path relative to the account's home (the SFTP default dir) and let the remote PowerShell move it; this sidesteps drive-letter syntax entirely.
- **Key file permissions:** Win32-OpenSSH refuses private keys whose ACL is too open. A `tempfile`-created key in the user's temp dir normally inherits a user-only ACL; add a live check on WFLWVM01 (MEDIUM, unverified).
- **Do not place snapshots inside `data/config/resources/`**: every subdirectory there is a collection. Use a sibling directory (for example `data/config/.ign-deploy/<run-id>/`).
- **Client availability:** Linux runner image for D12 (`onprem-linux`) must include `openssh-client`; WFLWVM01 needs the OpenSSH Client Windows capability (`ssh -V`). Preflight: `ign deploy` fails fast with a clear message if `ssh`/`sftp` are not found.

## 2. YAML manifest parsing

**Recommendation: `yaml_serde = "0.10"`.**

| Crate | Version | MSRV | Status | Verdict |
|-------|---------|------|--------|---------|
| `yaml_serde` | 0.10.7 (2026-08-18) | 1.82 | Official YAML-org fork of serde_yaml, 6 releases in 2026, 2.4M recent downloads | **Use** |
| `serde-saphyr` | 1.3.0 (2026-09-16) | **1.89** for 1.2.0+ | Pure Rust, panic-free, parsing budgets, no `Value` DOM, best error snippets, 7.3M recent downloads | Best technically; blocked by MSRV. With resolver 3 a 1.88 workspace silently lands on 1.1.0 (declares no MSRV; I confirmed it compiles on 1.88) which is a poor foundation. Revisit if/when the workspace MSRV moves to 1.89 |
| `serde_norway` | 0.9.42 (2024-12-21) | 1.71.1 | Fork of serde_yaml on `unsafe-libyaml-norway`; no release in ~10 months | Fine fallback, but `yaml_serde` is more active |
| `serde_yml` | 0.0.13 | 1.85 | **Unsound** (RUSTSEC-2025-0068, no patched version); 0.0.13 is a shim forwarding to `noyalib` | **Avoid** |
| `serde_yaml` | 0.9.34+deprecated | 1.64 | Archived March 2024 | Avoid in new code |
| `serde_yaml_ng` | 0.10.0 (2024-05) | 1.64 | Stale; still on unmaintained `unsafe-libyaml` | Avoid |
| `yaml-rust2` | 0.13.0 | 1.85 | Untyped low-level parser, no serde | Wrong layer |
| `noyalib` | 0.0.57 | 1.86 | Pre-1.0, single maintainer | Not for a prod deploy tool yet |

Usage rules for the manifest types: every struct `#[serde(deny_unknown_fields)]`; a top-level `schema_version: 1` field checked first; no YAML anchors/merge keys (serde_yaml-lineage crates do not apply `<<` merges automatically); parse errors go through the existing `CoreError::InvalidInput` path so the frozen envelope/exit taxonomy is untouched (YAML parse errors are user input errors, not new slugs).

## 3. Byte-compatible Gson JSON writing

### What the real gateway output looks like (measured, not assumed)

Test corpus: all 183 `*.json` files under `whk-environment-orchestration/gw-config/` (a real 8.3.6 `core` snapshot, 91 `config.json` + 91 `resource.json` + 1 `config-mode.json`).

| Finding | Evidence | Consequence |
|---------|----------|-------------|
| Indent is 2 spaces, `"key": value`, empty containers `[]`/`{}`, **no trailing newline** | 178/183 reproduced byte-for-byte by an order-preserving writer | serde_json `PrettyFormatter::with_indent(b"  ")` already matches |
| Gson HTML-safe escaping is on: `<`, `>`, `&`, `=`, `'` are `\u003c \u003e \u0026 \u003d \u0027` (lowercase hex). Observed: 184x `\u003c`, 183x `\u003e`, 35x `\u003d`, 21x `\u0027`; **zero** literal occurrences in any file | grep across the corpus | Custom escaper required. `\u2028`/`\u2029` are also escaped by Gson (standard Gson behavior, not in this corpus) |
| Non-ASCII is written raw (UTF-8), `/` is not escaped | corpus + Gson semantics | serde_json default already matches |
| **Key order is insertion order, NOT sorted.** `resource.json` is always `scope, version, restricted, overridable, files, attributes` (0/91 sorted); `attributes` is `lastModification, uuid, lastModificationSignature, enabled`; `security-properties/config.json` has non-alphabetical keys | 91/91 resource.json fail under sorting; 1 config.json fails under sorting | **Spec §4 "sorted keys" is wrong for gateway-produced files.** Sorting would make `render` rewrite every resource.json. Preserve source order; use the declared field order only for files `ign` authors |
| Not one canonical format even on a single gateway: 3-4 files are compact one-liners (Cirrus Link namespace-server-set, EAM agent-group, `quickstart`), 2 end with `\n` (`config-mode.json` from `POST /mode`, `security-properties`) | corpus | Different writers (module code, API) produce different bytes. Render must be **passthrough-by-default** |
| No floats in the corpus | corpus | Float lexeme fidelity (Gson `1.0E10` vs ryu `1e10`) is an edge; handled by the idempotence guard below |

### Recommendation: small local `ordered_json` module + custom `Formatter`; NOT a hand-written lexer/parser, NOT `preserve_order`

serde_json's own `PrettyFormatter` plus a wrapper overriding `write_string_fragment` is sufficient (verified: 178/183 byte-identical; the remaining five differ only by trailing newline or compact layout, i.e. a different writer). Key order needs an order-preserving tree: a 6-variant `Node` enum with `Vec<(String, Node)>` objects, a ~20-line `Deserialize` visitor, and `Serialize` via `serialize_map` in order. Total ~100 lines, zero new deps. This exact code was compiled and run on rustc 1.88 against the corpus.

```rust
struct Gson<'a>(PrettyFormatter<'a>);            // PrettyFormatter::with_indent(b"  ")
impl Formatter for Gson<'_> {
    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, w: &mut W, frag: &str) -> io::Result<()> {
        // for '<' '>' '&' '=' '\'' U+2028 U+2029 write  \u{:04x}; copy other runs verbatim
    }
    // forward begin/end_array/object, *_key/_value, write_char_escape to self.0
}
```

serde_json already emits `\b \f \n \r \t`, `\"`, `\\` and `\u00xx` (lowercase) for control characters exactly as Gson does.

**Canonicalization rule (the part that prevents corruption): rewrite a file only if `write(parse(bytes)) == bytes`; otherwise it is "opaque" and is copied verbatim.** The writer is used for (a) injecting `attributes.uuid` into new `resource.json` files, (b) emitting resources `ign` authors from scratch, (c) an optional `render --canonicalize`. A float or a trailing-newline file that fails the self-check is passed through, never silently reformatted. `plan`/`verify` compare **semantically** (parsed, key-order-insensitive) with sha256 of bytes as the secondary signal.

**Why not the alternatives:**

| Alternative | Why not |
|-------------|---------|
| `serde_json` `preserve_order` feature | Cargo feature unification makes it global: every `json!`/`Value` object in the CLI (959 `json!(` sites, frozen envelope goldens, MCP catalog) switches from sorted to insertion order. A contract-breaking blast radius for a local need. |
| `arbitrary_precision` | Global too; known to break `untagged`/`flatten` deserialization. Not needed given the idempotence guard. |
| Sorted keys (as spec §4 says) | Contradicted by 91/91 real `resource.json`. |
| Hand-written JSON lexer/parser | Reimplements what serde_json does correctly; larger surface to fuzz for no gain. |
| `serde_json` default `to_string_pretty` | Lacks HTML-safe escaping; every `<`/`=`/`'` in a config would differ from the gateway's bytes and show up as drift. |

**Capture-first flags (project discipline):** (1) the native 8.3 tag files (`tags.json`, `udts.json`, `unary-resource.json`) are not in the corpus (spike fixtures are synthetic compact JSON); capture a real tag directory and run the same corpus test before committing to a tag-file policy. (2) Whether the gateway rewrites `resource.json` (adds `lastModification*`) after loading a hand-written file. Both belong in a live-captures doc before code.

**Test plan:** commit ~10 representative real files as fixtures (pretty, compact, trailing-newline, `\u003d`-bearing, nested arrays) with a byte-identity golden; a proptest that `write(parse(write(x))) == write(x)`; a sabotage test that the self-check demotes a file with `1e3` to passthrough.

## 4. Structured diff for `plan`

**Recommendation: hand-rolled tree diff over `serde_json::Value` + `similar` for the human preview. No `json-patch`.**

- **Resource level (set diff, no library):** key = `(module, type, name)`; classify `Added | Changed | Removed | Unchanged` between render and gateway read-back. "Still shadowed in `core`" is a key-set intersection between the `production` and `core` listings. "Destructive" = any `Removed` plus any tag-definition removal (spec §8: runtime values are lost).
- **Leaf level (~50 lines):** `fn diff(path: &mut Vec<Seg>, old: &Value, new: &Value, out: &mut Vec<Change>)` emitting `{path (JSON Pointer), op: add|remove|replace, old, new}`. Objects compare key-wise (`Value` is a `BTreeMap`, so output order is deterministic, which suits the frozen JSON contract and byte-stable golden tests); arrays compare by index because config arrays are order-significant. Ignore gateway-owned `resource.json` `attributes.lastModification` and `lastModificationSignature`. Redact values at secret-looking paths in `plan` output.
- **Human preview:** `similar::TextDiff::from_lines(old_pretty, new_pretty)` unified diff, truncated to N lines per file in the refusal/preview text (consistent with `preview_then_confirm` where the refusal message IS the preview).

| Option | Version | MSRV | Verdict |
|--------|---------|------|---------|
| `similar` | 3.2.0 (already locked, 57M recent downloads) | 1.85 | Use for text preview |
| `json-patch` | 4.2.0 | not declared | Skip: produces RFC 6902 patches meant to be *applied*; naive array diff; extra dep tree for output we shape ourselves anyway |
| `imara-diff` | 0.2.0 | 1.71 | Skip: faster text diff than needed; `similar` already in tree |
| `diffy` | 0.5.2 | 1.85 | Skip: second text-diff crate for no gain |

## 5. UUID v4

**Recommendation: do not add `uuid`; hand-roll over the declared `getrandom 0.2`.**

```rust
fn new_uuid_v4() -> Result<String, getrandom::Error> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b)?;
    b[6] = (b[6] & 0x0f) | 0x40;      // version 4
    b[8] = (b[8] & 0x3f) | 0x80;      // RFC 4122 variant
    Ok(format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7],b[8],b[9],b[10],b[11],b[12],b[13],b[14],b[15]))
}
```

Format confirmed against real gateway `attributes.uuid` values: lowercase, hyphenated 8-4-4-4-12 (for example `da58d9d1-080b-4891-905c-6cf54df93044`). Why not the crate: `uuid` 1.27+/1.28.0 declare **MSRV 1.89**, so it is already past our 1.88 floor (resolver 3 would quietly pick 1.26.1; the lockfile has 1.24.1 only for non-host targets via the keyring stack), and its `v4` feature drags another `getrandom` major. Eight lines plus a version/variant-bit unit test is cheaper than managing that. Determinism for tests: take the RNG as `impl FnMut(&mut [u8])` so goldens can inject a fixed source. New UUIDs are generated once at first render of a resource and must be **persisted into the declared tree** (committed), never regenerated per render, or every plan shows churn; gateway-bootstrapped resources keep the gateway's own UUIDs.

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Remote transport | system `ssh`/`sftp` via `tokio::process` | `russh` 0.63.1/0.64.1 + `russh-sftp` 3.0.1 | +124 crates incl. pre-release crypto; MSRV 1.89 for current; ignores `~/.ssh/config`/`IdentityAgent`; Pageant-only agent on Windows |
| Remote transport | system OpenSSH | `ssh2` 0.9.6 | C dependency (libssh2 + OpenSSL); WinCNG ed25519 gap; contradicts the repo's no-C-toolchain stance |
| Remote transport | system OpenSSH | `openssh` 0.11.6 | Unix-only; WFLWVM01 is a supported client |
| File upload | single archive via `sftp -b put`, remote PS does extract + `robocopy /MIR` | per-file `scp -r` / tar piped through ssh stdin | `scp` protocol differs across Win32-OpenSSH versions; binary stdin through PowerShell is fragile; many files means many round-trips inside the lock window |
| YAML | `yaml_serde` | `serde-saphyr` | MSRV 1.89 (revisit on MSRV bump) |
| JSON writer | local `ordered_json` + `Formatter` | `serde_json/preserve_order` | Global blast radius on the frozen contract |
| Diff | hand-rolled `Value` diff + `similar` | `json-patch` | RFC 6902 apply semantics not needed |
| UUID | 8 lines over `getrandom 0.2` | `uuid` crate | MSRV 1.89 on latest; extra getrandom major |

## What NOT to Add

- `russh`, `russh-sftp`, `ssh2`, `openssh`, `openssh-sftp-client`, `ssh-key`, `libssh-rs`: no Rust SSH stack.
- `serde_yml`, `serde_yaml`, `serde_yaml_ng`: unsound / archived / stale.
- `uuid`, `json-patch`, `imara-diff`, `diffy`, `walkdir`, `ignore`/`globset`: each solves something that existing deps or ~50 lines already cover.
- `serde_json` features `preserve_order` / `arbitrary_precision` at workspace level.
- A second TLS/crypto provider (aws-lc-rs) as a side effect of any of the above.

## MSRV check (workspace floor: Rust 1.88)

| Crate | Version | Declared MSRV | Fits 1.88? |
|-------|---------|---------------|------------|
| yaml_serde | 0.10.7 | 1.82 | Yes (compiled on rustc 1.88.0) |
| similar | 3.2.0 | 1.85 | Yes |
| serde_json | 1.0.151 | (already in tree) | Yes |
| getrandom | 0.2.17 | (already in tree) | Yes |
| uuid (not adopted) | 1.28.0 / 1.26.1 | 1.89 / 1.85 | No / Yes |
| russh (not adopted) | 0.64.1 / 0.63.1 | 1.89 / 1.85 | No / Yes |
| serde-saphyr (not adopted) | 1.3.0 / 1.1.0 | 1.89 / none declared | No / compiles |

Ecosystem MSRV is drifting to 1.89 (uuid, russh, serde-saphyr all crossed it in the last six weeks). None of the v1.6 picks require a bump, so do not couple a bump to this milestone; keep `resolver = "3"` (MSRV-aware) and let CI's stable job catch patch-level MSRV drift (the existing ratatui precedent in Cargo.toml).

## Installation

```toml
# [workspace.dependencies]
yaml_serde = "0.10"          # v1.6: manifest.yaml / targets/*.yaml / gateway-deps.yaml (read-only)
similar = { version = "3", default-features = false, features = ["text"] }   # v1.6: promoted from dev (snapbox) to runtime; already locked

# crates/ignition-core/Cargo.toml [dependencies]
yaml_serde = { workspace = true }
similar = { workspace = true }
# no other additions: tokio(process), base64, sha2, zip, getrandom, serde_json, tempfile already present
```

```bash
cargo add yaml_serde@0.10 -p ignition-core     # then move to workspace.dependencies for house style
cargo build -p ignition-cli --no-default-features   # one of CI's five gates; run all five before pushing (WINDOWS.md #4)
```

## Confidence

| Claim | Confidence | Basis |
|-------|------------|-------|
| Crate versions, MSRVs, release dates | HIGH | crates.io API read 2026-10-10 |
| `yaml_serde` builds on 1.88 and enforces `deny_unknown_fields`, line/col errors, strict bools | HIGH | compiled and run in `rust:1.88` |
| russh+ring tree size (+124 crates), builds on 1.88 at 0.63.1 | HIGH | `cargo tree`/`cargo check` in `rust:1.88`, diffed against the repo's own tree |
| Gson escaping, 2-space pretty, no trailing newline, insertion key order, 178/183 byte-identical | HIGH | Rust prototype run against 183 real files (also cross-checked with an independent Python emulation) |
| Spec §4 "sorted keys" is incorrect | HIGH | 0/91 resource.json sorted; reproducible |
| Win32-OpenSSH stdin-NUL hang, CLIXML on stderr, WinCNG lacks ed25519, russh Pageant-only | MEDIUM-LOW | web search / library docs, single-source; mitigations are cheap and harmless if the issue is stale |
| Native tag-file byte format; whether the gateway rewrites loaded `resource.json`; SFTP remote path syntax; Windows key-file ACL behavior | LOW (unverified) | Needs live capture on WIGNVM01 / a rig before planning depends on it |

## Sources

- crates.io API (versions, MSRV, dependencies, downloads): yaml_serde, serde-saphyr, serde_norway, serde_yml, serde_yaml, serde_yaml_ng, yaml-rust2, noyalib, libyaml-rs, russh, russh-sftp, ssh2, openssh, similar, json-patch, imara-diff, diffy, uuid (retrieved 2026-10-10) - HIGH
- https://github.com/yaml/yaml-serde (maintained fork by the YAML organization) - HIGH
- Context7 `/bourumir-wyngs/serde-saphyr` (README states Rust 1.89+) and `/eugeny/russh` (Pageant-only Windows agent; `check_known_hosts`) - MEDIUM
- https://docs.rs/openssh/latest/openssh/ ("only works on unix", ControlMaster mux) - MEDIUM
- https://rustsec.org/advisories/RUSTSEC-2025-0068 (serde_yml unsound) - MEDIUM
- https://lists.mindrot.org/pipermail/openssh-bugs/2019-February/020019.html (Win32-OpenSSH hang with NUL stdin) - LOW/MEDIUM
- https://jpassing.com/2024/05/24/using-libssh2-with-cng-and-ecdsa and the jj changelog note on libssh2 WinCNG/ed25519 - LOW/MEDIUM
- Local evidence: `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/gw-config/` (183 real gateway JSON files), spec `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md`, `~/.ssh/config` (per-host blocks + `IdentityAgent`), `Cargo.toml`/`Cargo.lock`/`cargo tree` of this repo, `rig/compose.rs` (process-seam precedent)
