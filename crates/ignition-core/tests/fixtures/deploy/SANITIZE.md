# Deploy corpus sanitization (Phase 21, plan 21-01)

## Why

`ignition-cli` is a public repository. The Phase 21 render corpus is a snapshot of a real
WHK dev gateway configuration tree. Left as-is it carries real Embedded secret ciphertext,
an Entra tenant GUID, an Entra client GUID, an internal IPv4 address and a tenant mail host
(RESEARCH Pitfall 1). `sanitize.py` rewrites exactly those values by pattern and copies
everything else byte-for-byte. No original value appears in the script, in this document,
in any commit message or in the plan summary.

## Source and identity

- Source: the `gateway-config-spec/gw-config` directory of the `whk-environment-orchestration`
  worktree under `/Users/pmannion/data/worktrees/`. Snapshot date: 2026-10-09.
- Identity: 195 regular files (183 `.json`, 8 `.css`, 4 `.jar`), no symlinks, no other file kind.
- Aggregate SHA-256 manifest digest:
  `6477b7e5e010cbe56f1ded82b080108c31763e357d5c604d91bca5f5b8f0bfcf`
- Digest command, run from inside the source directory (it hashes the byte-sorted `shasum`
  listing, so it covers every file's bytes and relative path; it prints the digest followed by `  -`):

  ```
  find . -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 | shasum -a 256
  ```

A re-run against a source with any other digest is a different corpus and needs a new review.

## Source of truth

The committed `corpus/` directory plus this file is the source of truth for the corpus from now
on. The external worktree path is needed by no test and no later plan (`deploy_fixtures`,
`deploy_corpus`, the corpus tests of 21-07 and 21-08, and CI on all three OS legs read only the
committed corpus). Only a deliberate re-sanitization from a new source needs the external path.

## Re-run command

From the repository root, with an absent or empty destination (the script refuses a non-empty one):

```
python3 -I crates/ignition-core/tests/fixtures/deploy/sanitize.py \
  /Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/gw-config \
  crates/ignition-core/tests/fixtures/deploy/corpus
```

The script is stdlib-only and deterministic: the same source yields the same tree.

## Rules

Substitution is exact textual replacement on the original text, so key order, escapes,
indentation and the absence of a trailing newline survive.

| Rule | Target | Replacement |
|------|--------|-------------|
| S1 | Every string member of the `data` object of an object whose `type` is `Embedded` | `SANITIZED` followed by `A` characters up to the original length (a value shorter than 9 characters becomes exactly `SANITIZED`) |
| S2 | Every GUID inside a string containing `login.microsoftonline.com/`, and every GUID-valued `clientId` or `tenantId` member | `00000000-0000-4000-8000-0000000000NN`, NN = 01, 02, ... in first-seen order over the byte-sorted walk; one source GUID always maps to one fake |
| S3 | Every dotted-quad inside a JSON string value that is not in 127.0.0.0/8, 0.0.0.0, 224.0.0.0/4 or an RFC 5737 range | `192.0.2.<last octet of the original>` |
| S4 | A string value ending in `.mail.protection.outlook.com` or `.onmicrosoft.com` | `smtp.example.com` |
| S5 | Each `*.jar` | Stand-in with the same file name: ASCII `IGN-DEPLOY-STAND-IN-JAR` followed by bytes 0x00 0x0D 0x0A 0x00 0x0D (D-01: a binary member that contains CR, which render must not refuse) |
| S6 | Everything else: CSS, every untouched JSON byte, `config-mode.json`, file names, directory names including `WHK01_*` resource names | Byte-for-byte copy |

Before exiting 0 the script verifies its own output: identical key sequences at every level,
every differing string is one S1-S4 replaced, untouched files are byte-identical, and a final
census finds no unmarked Embedded value, no IPv4 outside the allowed ranges, no non-fake Entra
GUID and no tenant mail host. It prints per-rule counts only, never values.

## Observed counts (run of 2026-10-13)

| Counter | Count |
|---------|-------|
| Files total | 195 |
| S1 files touched | 10 |
| S2 distinct GUIDs | 2 |
| S3 IPv4 occurrences | 5 |
| S4 mail hosts | 1 |
| S5 jars | 4 |
| Copied verbatim (S6) | 175 |

## Deliberately kept

Resource and directory names, `admin`-style usernames, the multicast address,
`jdbc:postgresql://db:5432/...` docker-network URLs, and resource `uuid` values (random gateway
ids, not credentials).

## Enforcement

The guard test `crates/ignition-core/tests/deploy_fixtures.rs` enforces S1-S5 on every file under
`fixtures/deploy/`, including fixtures captured or added by later plans: Embedded values carry the
marker, IPv4 literals are reserved ranges only, Entra identifiers are fake, no tenant mail hosts,
no CR in any text fixture, and the corpus inventory is 183 `.json`, 8 `.css`, 4 `.jar` (each jar at
most 64 bytes with a 0x00 and a 0x0D byte). `.gitattributes` marks `fixtures/deploy/**` as `-text`
and `*.jar` as `binary` so git for Windows does not CRLF-convert fixtures at checkout.
