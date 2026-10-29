---
phase: 21-deploy-render
reviewed: 2026-10-13T15:55:30Z
depth: standard
files_reviewed: 35
files_reviewed_list:
  - .gitattributes
  - Cargo.toml
  - README.md
  - crates/ignition-cli/src/cli.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/src/mcp.rs
  - crates/ignition-cli/src/render.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - crates/ignition-cli/tests/contract_stdout_purity.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
  - crates/ignition-core/Cargo.toml
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/actions/mod.rs
  - crates/ignition-core/src/deploy/layout.rs
  - crates/ignition-core/src/deploy/lint.rs
  - crates/ignition-core/src/deploy/manifest.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/node.rs
  - crates/ignition-core/src/deploy/render.rs
  - crates/ignition-core/src/deploy/rules.rs
  - crates/ignition-core/src/deploy/target.rs
  - crates/ignition-core/src/deploy/tree.rs
  - crates/ignition-core/src/deploy/uuid.rs
  - crates/ignition-core/src/deploy/yamlio.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/src/lib.rs
  - crates/ignition-core/tests/deploy_corpus.rs
  - crates/ignition-core/tests/deploy_fixtures.rs
  - crates/ignition-core/tests/deploy_render.rs
  - crates/ignition-core/tests/fixtures/deploy/SANITIZE.md
  - crates/ignition-core/tests/fixtures/deploy/extract_openapi.py
  - crates/ignition-core/tests/fixtures/deploy/sanitize.py
  - crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh
  - crates/ignition-tui/src/routes.rs
  - .claude/CLAUDE.md
findings:
  critical: 0
  warning: 8
  info: 11
  total: 19
status: issues_found
---

# Phase 21: Code Review Report

**Reviewed:** 2026-10-13T15:55:30Z
**Depth:** standard
**Files Reviewed:** 35
**Status:** issues_found

## Summary

The offline render is carefully built. I traced every path that builds an error, warning or
envelope (yamlio scrub, lint `SecretScan`, node parser errors, render findings, `--out` refusals)
and found no route by which a secret value reaches output. `SecretScan` and `DeployFinding` have no
value-bearing field, `scrub_values` covers the serde and yaml_serde value-echo shapes (including
float and integer backtick forms and the tag form), and the corpus fixtures contain no unsanitized
GUIDs, hosts, mail hosts or Embedded values (grepped). Determinism holds: `BTreeMap`/`BTreeSet`
throughout, entries sorted by name bytes, no clock or RNG in `deploy/`. `--env`, `gateway`, `mode`,
type halves and tag providers all go through `component_problem`, and `RelPath` plus the Windows
lints stop traversal before `write_out` joins a path.

I found no BLOCKER. The weaknesses are in four places:

- The secret lint fails open for several value shapes (WR-01, WR-02).
- `write_out` is not crash-safe, although the README describes the interrupted case (WR-03).
- A wrong error hint and a case-insensitive-filesystem determinism hole (WR-04, WR-05).
- Two fixture and tooling scripts carry destructive or leaky failure modes (WR-06, WR-07, WR-08).

Verification was static; no cargo build or test was run (disk constraint).

## Warnings

### WR-01: Key-name secret rule only fires for non-empty strings

**File:** `crates/ignition-core/src/deploy/lint.rs:97-134` (rule at 55-58)
**Issue:** `key_hit` is consumed only by the `Node::Str` arm (and arrays of strings). A member named
`password` or `secret` whose value is an object (`{"value":"hunter2"}`), a number, a boolean or an
array of objects is walked without ever being reported, because `hit` is recomputed per child key
and ignored for `Node::Obj`/`Node::Lit`. In a type the OpenAPI does not describe (the Cirrus Link
MQTT types are the only ones with rows today), this is the only plaintext defence, and it is the
path the module doc calls fail-closed. The needles are also only `password` and `secret`, so
`passwd`, `passphrase`, `token`, `apiKey`, `privateKey` and `credential` never match.
**Fix:**
```rust
// in walk(): decide on the member before recursing
if hit_by_key && !matches!(value, Node::Lit(l) if l == "null") && !is_referenced(value) {
    match value {
        Node::Str(s) if s.is_empty() => {}
        Node::Arr(a) if a.is_empty() => {}
        _ => { self.key_name_hit(&child); continue; }
    }
}
```
and widen `key_looks_secret` to a small const needle list (`password`, `passwd`, `passphrase`,
`secret`, `token`, `apikey`, `privatekey`, `credential`).

### WR-02: A Referenced object's `data` is unvalidated and switches the key-name rule off

**File:** `crates/ignition-core/src/deploy/lint.rs:46-51, 105-120, 138-145`
**Issue:** `is_referenced` only checks `type == "Referenced"` and that `data` is an object. Inside
that `data`, `in_ref_data` disables the key-name rule for the whole subtree (line 118) and
`schema_value` accepts any such object at a secret-typed path. So
`{"type":"Referenced","data":{"providerName":"p","secretName":"s","password":"hunter2"}}` passes,
as does a Referenced wrapper placed anywhere to hide a `password` member. This is fail-open on the
exact property the lint exists to guarantee (no plaintext secret in a tree that is later written
with `--out`).
**Fix:** Require `data` to hold exactly the string members `providerName` and `secretName`
(CD7), and treat anything else as `plaintext`:
```rust
fn is_referenced(node: &Node) -> bool {
    matches!(node.get("type"), Some(Node::Str(t)) if t == "Referenced")
        && matches!(node.get("data"), Some(Node::Obj(m))
            if m.len() == 2
                && m.iter().all(|(k, v)| matches!(k.as_str(), "providerName" | "secretName")
                    && matches!(v, Node::Str(_))))
}
```

### WR-03: `write_out` is not crash-safe and can leave a stale valid marker over mixed content

**File:** `crates/ignition-core/src/deploy/tree.rs:412-425, 456-474`
**Issue:** The doc and README say an interrupted write is safe because the marker goes last. That
holds for `Create` and `Empty`, but:
1. In `Refresh`, the old `render.json` stays in place while listed files are replaced one by one.
   An interruption (or any `?` failure mid-loop) leaves new bytes in some files under a marker
   whose `render_sha256` and per-file hashes describe the old render. Phase 22 reads `render.json`
   as `plan --base`; any reader that trusts the marker without re-hashing gets a wrong base.
2. `write_replacing` leaves `<name>.ign-render-tmp` behind when `write_all` or `rename` fails
   (disk full, Windows sharing violation). The leftover is then an unlisted file, so every later
   run is refused with "not listed in render.json" even though ign created it.
3. No failure path removes the files this run created with `create_new`, so a failed first render
   forces the user to delete the directory by hand.
**Fix:** Before the first replacement in `Refresh`, atomically invalidate the old marker (rename
`render.json` to `render.json.ign-render-tmp`, or write an `in_progress: true` marker). On error,
best-effort remove the `.ign-render-tmp` just created and the files in a `created: Vec<PathBuf>`
(they are ign-owned by construction); never touch anything else.

### WR-04: `invalid_input` hint is wrong for every `deploy render` refusal

**File:** `crates/ignition-core/src/error.rs:1109-1128` (reached by `tree.rs:194-201` and
`actions/deploy.rs:66-70`)
**Issue:** `deploy render` raises `InvalidInput` for a bad `--env` and for every `--out` refusal,
but the generic `InvalidInput` hint is "fix the input source — a readable file path via --file, or
`-` to pipe the content on stdin". There is no `--file` on this verb. The hint rides in the JSON
envelope (`error.hint`), so an agent following it is misdirected, and the `--out` message already
ends with the real remedy. The new `deploy_slugs_are_exit_two` test only covers the two new slugs,
so this is untested.
**Fix:** Add a contextual branch keyed on a reason prefix, as `LOSS_GATE_REFUSAL_REASON_PREFIX`
does, e.g. `reason.starts_with("--out ") || reason.starts_with("--env ")` returning "choose an
absent or empty --out directory, or a safe single-component --env".

### WR-05: Render is not deterministic across case-insensitive and case-sensitive filesystems

**File:** `crates/ignition-core/src/actions/deploy.rs:72-83, 164-183`
**Issue:** `targets/<env>.yaml` and `gateways/<target.gateway>` are opened by path, not matched
against a directory listing. On macOS (APFS default) `--env dev` happily reads `targets/Dev.yaml`
and `gateway: Tracer-GW` resolves `gateways/tracer-gw`; on Linux CI both are `deploy_tree_invalid`.
Worse, `input.gateway` is the target's spelling, and it feeds `name_based_uuid`, so a macOS render
mints different uuids than the same repo would after the spelling is fixed. This breaks the
documented invariant "same input yields the same output or the same error" for exactly the
works-on-my-Mac class of repo bug. The Reader already compares names byte-exactly, so the
inconsistency is in the entry points only.
**Fix:** After `symlink_metadata` succeeds, confirm the exact-case name by listing the parent
(`read_dir(parent).any(|e| e.file_name() == name)`) and return `does not exist` otherwise; do this
for `targets/<env>.yaml`, `gateways/` and `gateways/<gateway>`.

### WR-06: `sanitize.py` writes unsanitized output before its self-check fails

**File:** `crates/ignition-core/tests/fixtures/deploy/sanitize.py:252-289`
**Issue:** Each file is written to `DST` inside the loop (line 283) and the `failures` list is only
acted on after all files are written. If the census flags a residual (for example an Embedded value
containing `=`, which the source holds as `=`, so the exact-token replace at line 149 never
matches), the script exits 1 but `DST` already contains the real secret. `DST` is documented as the
in-repo `corpus/` directory in a public repository, and the script then refuses to re-run because
`DST` is non-empty, which pushes the operator toward committing or hand-editing.
**Fix:** Sanitize into `tempfile.mkdtemp()` next to `DST`, and `os.replace`/move into place only
when `failures` is empty; otherwise delete the temp tree.

### WR-07: `capture.sh` `rm -rf` guard is bypassable with `..`

**File:** `crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh:425-431`
**Issue:** The guard `"$HOME/.cache/ign-deploy-rig/"?*` is a glob prefix match, so
`IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/../../Documents` passes and `/bin/rm -rf` then
deletes it. The variable comes from the environment, so a stale export in a shell profile is enough.
**Fix:**
```bash
case "$BIND21" in
  *..* ) echo "REFUSING: bind dir contains '..'" >&2; exit 2 ;;
  "$HOME/.cache/ign-deploy-rig/"?*) ;;
  *) echo "REFUSING: ..." >&2; exit 2 ;;
esac
```
Better, resolve with `realpath -m` first and compare the resolved prefix.

### WR-08: `assert_local` accepts a non-local host when `BASE` contains `#` or `?`

**File:** `crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh:27-40`
**Issue:** The host is taken from the text after the last `@` in the authority. For
`BASE=http://evil.example#@localhost:18290` the "authority" is `evil.example#@localhost:18290`,
`hostport` becomes `localhost:18290`, the check passes, and curl then treats `#` as a fragment and
sends `X-Ignition-API-Token` to `evil.example`. The script's header says tokens are localhost-only.
**Fix:** Cut the authority at the first of `/ ? #` before extracting userinfo:
```bash
authority=${rest%%[/?#]*}
```
and reject any `BASE` containing whitespace, `\` or `@`.

## Info

### IN-01: MCP tool `deploy_render` can create directories and write files anywhere without `confirm`

**File:** `crates/ignition-cli/src/mcp.rs:861-884`, `crates/ignition-core/src/deploy/tree.rs:443-447`
**Issue:** The test pins "read-only, unguarded", but `--out` calls `create_dir_all(parent)` for any
absent path and writes attacker-influenced bytes (an agent chooses both `--repo` and `--out`). The
ladder prevents overwrite, not creation, and Decision D accepts this. Worth restating in the threat
model, because the tool description advertises it as read-only.
**Fix:** Either drop `create_dir_all` for parents (require the parent to exist) or add the `--out`
write to the guarded set when invoked over MCP.

### IN-02: IO failures during `--out` are `internal` (exit 1), not a usage error

**File:** `crates/ignition-core/src/deploy/tree.rs:398-408`
**Issue:** Permission denied, a regular file as parent, or a read-only filesystem become
`CoreError::Internal` with the raw `io::Error`. The README says `--out` problems are `invalid_input`
and exit 2; an agent will treat exit 1 as a bug in ign.
**Fix:** Map `PermissionDenied`, `NotADirectory`, `ReadOnlyFilesystem`, `StorageFull` to `refuse(...)`.

### IN-03: Check phase and write phase are separate; intermediate directories are followed

**File:** `crates/ignition-core/src/deploy/tree.rs:216-355, 456-474`
**Issue:** After `check_out` verifies no symlinks, `create_dir_all` and `OpenOptions::open` re-resolve
paths. `create_new` refuses a final symlink, but a directory component swapped for a symlink in the
window is followed. Needs a local attacker with write access to the target, so low risk; the README
promises "never follows links".
**Fix:** On Unix, open `out` once and use `openat`-style creation relative to that fd, or re-run the
cheap `symlink_metadata` check on each parent right before creating files.

### IN-04: Prior-render check is strict about unrelated files

**File:** `crates/ignition-core/src/deploy/tree.rs:292-310, 326-332`
**Issue:** A `.DS_Store` created by Finder, a `.git` directory, or an editor swap file in a previous
render directory makes the next `--out` refuse with "not listed in render.json". It is documented,
but users who commit the rendered tree will hit it constantly.
**Fix:** Optional: ignore a fixed list (`.DS_Store`, `.gitkeep`) in `walk_out`, or have the error
name the exact file and the one-line remedy.

### IN-05: Windows path budget uses only the selected environment's target

**File:** `crates/ignition-core/src/deploy/lint.rs:260-268, 385`
**Issue:** `resources/` is shared between environments, and the doc says the default data directory
always counts for that reason, but another environment's longer `mode` or Windows `data_dir` is not
considered. A tree renders cleanly for `--env dev` and fails for `--env production`.
**Fix:** Accept the limitation in the doc comment, or take the max over all `targets/*.yaml`.

### IN-06: Windows name lints miss superscript device names, `CONIN$`/`CONOUT$`, and Unicode normalization

**File:** `crates/ignition-core/src/deploy/lint.rs:227-230, 448-470`
**Issue:** `COM¹ COM² COM³ LPT¹ LPT² LPT³`, `CONIN$` and `CONOUT$` are reserved by current Windows.
Case-fold collision detection uses `to_lowercase`, which does not catch NFC/NFD pairs that APFS
treats as one name. Both are fail-open in a lint documented as fail-closed.
**Fix:** Add the extra stems, and group siblings by `name.nfc().collect::<String>().to_lowercase()`
if a normalization dependency is acceptable; otherwise document the gap.

### IN-07: `component_problem` does not reject `:` or control characters

**File:** `crates/ignition-core/src/deploy/mod.rs:29-43`, `actions/deploy.rs:80-83`
**Issue:** On Windows, `repo.join("gateways").join("C:foo")` replaces the whole path. It is blocked
today only because `require_real_dir(repo, "gateways/C:foo")` fails first (stream-name semantics).
`gateway` and `env` reach the filesystem and terminal output with no check beyond separators.
**Fix:** Reject `:` and `char::is_control` in `component_problem`, so safety does not depend on call
order.

### IN-08: Only `.json` members are scanned for secrets

**File:** `crates/ignition-core/src/deploy/render.rs:377-403`
**Issue:** Non-JSON members (`.properties`, `.xml`, `.py`, `.txt`) go straight to the output with no
scan. Reasonable for binary files, but the README says secrets "never" ship in a tree.
**Fix:** State in the README that non-JSON text is not scanned, or run a conservative
`password=`/`secret=` line scan over `TEXT_EXTENSIONS` members.

### IN-09: Findings reach the JSON envelope only as a truncated message string

**File:** `crates/ignition-core/src/error.rs:~800-845`
**Issue:** `DEPLOY_FINDINGS_SHOWN = 20` caps the text and the envelope has no structured findings
array, so an agent fixing a tree with 40 findings cannot see the rest and must iterate.
`EmbeddedSecretDetails::fmt` with 20 or more paths and any other finding also prints
`also N other tree finding(s): ; and N more` (stray `: ;`), since `room` is 0.
**Fix:** Add `details.findings: [{file,line,column,message}]` to the error envelope; special-case
`room == 0` in the Display.

### IN-10: Doc inconsistencies and a mangled doc comment

**File:** `crates/ignition-core/src/actions/deploy.rs:51-52`, `crates/ignition-cli/src/render.rs:1387-1393`,
`crates/ignition-core/src/deploy/node.rs:52-56`
**Issue:**
- `uuids_minted` is documented as "Collection paths" here but as `module/type/name` ids in
  `render.rs:81` and the README (line 2253).
- The new `render_deploy_render_human` was inserted into the middle of an existing doc comment
  block, so `render_lint_human` lost its doc and the deploy function carries the lint text.
- The `MAX_DEPTH` doc claims the build cannot cost O(depth x size), but `build` re-parses each
  subtree at every level, so worst case is about 128 x 16 MiB of scanning.

**Fix:** Correct the three comments (one-line edits).

### IN-11: `capture.sh` passes the API token on the curl command line

**File:** `crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh:70-71, 95, 148, 186, 395`
**Issue:** `-H "X-Ignition-API-Token: $tok"` is visible in `ps` to other local users for the life of
the request. Localhost and disposable tokens, so low risk.
**Fix:** Pass headers with `-H @<(printf ...)` or a `--config` file with 0600 permissions.

---

_Reviewed: 2026-10-13T15:55:30Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
