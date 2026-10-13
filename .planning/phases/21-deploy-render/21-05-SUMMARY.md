---
phase: 21-deploy-render
plan: 05
subsystem: deploy
tags: [render, json, gson, byte-fidelity, corpus, proptest, node-tree]

requires:
  - phase: 21-02
    provides: decision CD4 (gateway tag files are Gson pretty, raw UTF-8, HTML-safe escapes) and twelve captured tag files
  - phase: 21-04
    provides: render pipeline with named stages, RENDER_GOLDEN
provides:
  - "deploy::node: order-preserving Node tree, RawValue parser, Gson pretty writer, classify gate (GatewayFormat / Opaque), JSON_MAX_BYTES, MAX_DEPTH"
  - "render stage 5: every .json member classified, unparseable JSON refused with source paths, parsed-node side table, opaque_files filled"
  - "deploy_corpus.rs: corpus Gson census, captured-file census, whole-corpus byte-identical render"
  - "README Byte fidelity subsection"
affects: [21-06, 21-07, 21-08, 21-09]

tech-stack:
  added: []
  patterns:
    - "Raw-lexeme Node tree: numbers, true, false, null are never reformatted"
    - "Linear depth scan before the recursive build, instead of relying on serde_json's recursion limit"

key-files:
  created:
    - crates/ignition-core/src/deploy/node.rs
    - crates/ignition-core/tests/deploy_corpus.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/render.rs
    - crates/ignition-core/tests/deploy_render.rs
    - README.md

key-decisions:
  - "Nesting is capped by our own linear scan (MAX_DEPTH = 128), because serde_json's RawValue validation accepted 200-deep nesting"
  - "Stage 5 runs after the allowlist and collects findings from both, so one deploy_tree_invalid names every defect"
  - "The side table is a BTreeMap<RelPath, (JsonClass, Node)> that stages 6 and 7 will read; finish() already uses it for opaque_files"

patterns-established:
  - "Findings from tree content carry serde_json positions or key names only, never values (errors_never_echo_values)"

requirements-completed: []
requirements-partial: "DRND-05 stays Pending: see Requirement tracking"

actuals:
  tokens: 7900
  tasks: 2
  commits: 2

duration: 40min
completed: 2026-10-13
status: complete
---

# Phase 21 Plan 05: Gson writer, JSON classification and corpus byte fidelity Summary

**An order-preserving `Node` tree with a Gson-compatible pretty writer classifies every `.json` member as gateway-format or opaque; the 183-file corpus splits 178/5 exactly as predicted, all twelve captured gateway tag files are gateway-format, and the sanitized corpus renders byte-identical.**

## Accomplishments

- `deploy/node.rs`: `Node {Obj, Arr, Str, Lit}`, `parse`, `write`, `classify`, `JsonClass`, `JSON_MAX_BYTES` (16 MiB), `MAX_DEPTH` (128). Object members keep source order, number lexemes pass through (`2.50`, `1.0E10`, `-0.0`, `18446744073709551616`), a repeated key in one object is refused, non-ASCII is raw UTF-8 per CD4. `serde_json` features unchanged (`raw_value` only).
- Render stage 5: each collection path keeps its origin `gateways/<g>/<repo path>`; `.json` members (ASCII case-insensitive) go through `classify`; failures are findings named by source path and raised once with the allowlist findings; `opaque_files` is filled; bytes are never changed.
- `deploy_corpus.rs`: four tests (`corpus_gson_census`, `captured_tag_files_follow_cd4`, `corpus_render_is_byte_identical`, `render_refuses_unparseable_json`).
- README `#### Byte fidelity` under `### deploy render`.

## Census counts

Corpus: 183 `.json` files, 178 gateway-format, 5 opaque, 0 errors. The five (the plan guessed the two Cirrus Link names; the real ones are below):

- `config-mode.json` (trailing newline)
- `ignition/security-properties/config.json` (trailing newline)
- `ignition/quickstart/config.json` (compact)
- `com.cirruslink.mqtt.engine.gateway/namespace-server-set/Elecsys-Default Set/config.json` (compact)
- `com.cirruslink.mqtt.engine.gateway/namespace-server-set/Sparkplug B-Default Set/config.json` (compact)

Whole-corpus render: `config-mode.json`, the four denied types (`security-properties`, `security-levels`, `user-source`, `api-token`) and every resource directory whose `config.json` holds `"type": "Embedded"` are excluded; everything left renders with every file equal to its source bytes, `opaque_files` equal to quickstart plus the two namespace-server-set files, and `uuids_minted` empty.

## CD4 split

All twelve captured files under `fixtures/deploy/captured/` classify as `GatewayFormat`, none opaque, as CD4 states. The discriminating file `tag-definition/CAP21/FolderA/tags.json` carries `1.0E10`, `<` and `9007199254740993` (asserted in the test), so the writer reproduces Gson's HTML-safe escaping and raw non-ASCII on real gateway output.

## Task Commits

1. Task 1, `deploy::node` parser, writer, classify gate: `4096630`
2. Task 2, render stage 5, corpus gate, README: `8e2d40d`

## Requirement tracking

**DRND-05 stays Pending.** Its full text is: gateway-written files pass through byte-for-byte; files `ign` synthesizes use a Gson-compatible, order-preserving writer; proven by the corpus round-tripping byte-identical on Linux, macOS and Windows.

- Delivered: byte pass-through, the writer, and the corpus proof on macOS (host) with all tests green.
- Linux and Windows: `deploy_corpus` has no `cfg` gating and runs in the `cargo test --workspace` step (`.github/workflows/ci.yml` line 42) on every leg of the matrix (lines 11-17: `ubuntu-latest`, `macos-latest`, `windows-latest`); `.gitattributes` marks `fixtures/deploy/**` as `-text` so checkouts keep the bytes. That is the mechanism, but no CI run has happened yet, so the claim "byte-identical on three OSes" is not proven. 21-09 records the green three-leg run for the commit carrying `deploy_corpus`.
- Synthesized files (uuid injection through the writer) land in 21-07. Complete DRND-05 after both.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] serde_json did not enforce a nesting limit for RawValue validation**
- **Found during:** Task 1 (`classify_boundaries`: a 200-deep array classified successfully)
- **Issue:** The plan relied on serde_json's recursion limit from whole-document `RawValue` validation. It accepted 200 levels, and the recursive build would then be unbounded for hostile input (stack overflow, T-21-19).
- **Fix:** `max_depth` linear scan (string-aware) before the build, `MAX_DEPTH = 128`; error `nesting deeper than 128 levels`. Tests cover 129 (refused), 128 (accepted) and brackets inside strings (ignored).
- **Files modified:** crates/ignition-core/src/deploy/node.rs
- **Commit:** 4096630

**2. [Rule 1 - Bug] Existing test used non-JSON bodies in `.json` files**
- **Found during:** Task 2 (`cargo test --workspace`)
- **Issue:** `listing_format_is_pinned` (21-04) rendered `config.json` files holding `one` and `two`; stage 5 now correctly refuses them.
- **Fix:** bodies became the valid 3-byte documents `[1]` and `[2]`, keeping the `bytes() == 6` assertion; the digest logic is unchanged.
- **Files modified:** crates/ignition-core/tests/deploy_render.rs
- **Commit:** 8e2d40d

**3. Plan data correction.** The plan named the two Cirrus Link opaque files `Chariot SCADA` and `Default`; the corpus holds `Elecsys-Default Set` and `Sparkplug B-Default Set`. The counts (178/5) and the file kinds were right; only the names in the test constant were fixed.

**4. TDD note.** The behavior tests and implementation of Task 1 landed in one commit (`feat`), with no separate failing-test commit: the tests need the module to compile. The one genuine red was `classify_boundaries` failing on the depth case, which drove deviation 1.

**5. Determinism grep.** `grep -rnE 'HashMap|HashSet|SystemTime|Instant::now|thread_rng|getrandom' crates/ignition-core/src/deploy/` prints one line: `yamlio.rs:448` `Instant::now()` inside the 21-03 test `alias_bomb_is_refused` (test-only timeout guard, already recorded by 21-04). Production code is clean.

## Known Stubs

None.

## Threat Flags

None. T-21-19 (size cap before parse, depth scan before build, boundary tests), T-21-20 (duplicate keys refused, `duplicate_key_refused`, `render_refuses_unparseable_json`), T-21-21 (bytes pass through, rewrite gated, corpus census) and T-21-22 (`errors_never_echo_values`, and the refusal test asserts neither value appears) are covered.

## Verification

- `cargo test -p ignition-core --lib deploy::node`: 10 passed (including the proptest)
- `cargo test -p ignition-core --test deploy_corpus`: 4 passed
- `cargo test -p ignition-cli --test contract_deploy render_digest_golden`: passed, `RENDER_GOLDEN` untouched (`git diff` empty)
- `cargo test --workspace`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`: clean
- `grep 'preserve_order\|arbitrary_precision'` over both Cargo.toml: nothing; feature tree count 0
- `grep -c 'Byte fidelity' README.md`: 1

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/deploy/node.rs, crates/ignition-core/tests/deploy_corpus.rs
- FOUND commits: 4096630, 8e2d40d
