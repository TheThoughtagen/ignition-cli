---
phase: 21-deploy-render
plan: 03
subsystem: deploy
tags: [yaml_serde, strict-parse, error-taxonomy, manifest, target, four-place-rule]

requires:
  - phase: 20-config-api-client
    provides: boxed-payload error precedent (20-02 four-place landing)
provides:
  - "CoreError::DeployTreeInvalid / EmbeddedSecret (slugs deploy_tree_invalid, embedded_secret; exit 2) in all four places"
  - "deploy::yamlio::strict_parse with file:line:column, value-free errors"
  - "deploy::manifest::Manifest and deploy::target::Target schemas including verify.tag_probes (SF-8) and ssh block (SF-10)"
  - "deploy::component_problem safe path-component guard"
affects: [21-04, 21-05, 21-06, 21-07, 22, 25]

tech-stack:
  added: [yaml_serde 0.10.7, libyaml-rs 0.3.0 (transitive)]
  patterns:
    - "schema_version probe pass before the strict typed pass"
    - "semantic validation collects every finding into one deploy_tree_invalid"

key-files:
  created:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/yamlio.rs
    - crates/ignition-core/src/deploy/manifest.rs
    - crates/ignition-core/src/deploy/target.rs
  modified:
    - crates/ignition-core/src/error.rs
    - README.md
    - Cargo.toml
    - Cargo.lock
    - crates/ignition-core/Cargo.toml
    - crates/ignition-core/src/lib.rs

key-decisions:
  - "yaml_serde location() is 1-BASED for line and column; custom errors raised inside a value are positioned at the START OF THE ENCLOSING MAPPING, so the schema_version error is relocated to the `schema_version:` line, column 1"
  - "No pre-parse alias guard: yaml_serde has its own repetition limit and refuses the 9-level bomb as `repetition limit exceeded` in ~40 ms. A 1,024 alias-token count would not have caught it anyway (the bomb has 72 alias tokens)"
  - "libyaml-rs 0.3.0 and yaml_serde 0.10.7 both ship NO build.rs (`build = false` in Cargo.toml); nothing runs at build time beyond rustc (RESEARCH A12 closed)"
  - "Error messages scrub echoed values from invalid type/value (string, integer, float, boolean, character), unknown variant, and nested-enum tag messages; unknown field names a key and is kept"

patterns-established:
  - "DeployFinding {file, line, column, message}: positions only when real; missing/duplicate field report file only (yaml_serde reports 1:1 for them)"

requirements-completed: [DRND-02]  # DRND-08 (case-fold/Windows names/length) and DSURF-01 (verb + TUI row + family coverage) are NOT complete: only the CR guard and the slugs exist so far

actuals:
  tokens: 14700
  tasks: 3
  commits: 4

duration: 45min
completed: 2026-10-13
status: complete
---

# Phase 21 Plan 03: Deploy error slugs, strict YAML layer, manifest/target schemas Summary

**`deploy_tree_invalid` and `embedded_secret` landed under the four-place rule in one commit, then `yaml_serde`-backed `strict_parse` and the SF-8/SF-10 `Manifest`/`Target` schemas with positioned, value-free errors.**

## Accomplishments

- Both slugs exist in the error.rs header table, the variants (code/exit_code/hint/endpoint), `exit_code_mapping_enumerated` plus `EXIT_SLUG_LITERALS`, and README exit row 2, in commit c8800b4 (only error.rs and README.md). Payloads are boxed; `EmbeddedSecretDetails` holds paths and findings only, so no field can carry a secret value. Display caps at 20 entries (`and N more`).
- `strict_parse(file, bytes)` is pure: size cap (1,048,576 accepted, 1,048,577 refused), first CR with line and column, UTF-8 check, `schema_version` probe, then the strict parse. Missing/duplicate fields name the file only.
- `Manifest` (types, tag_providers, projects, verify.tag_probes) and `Target` (gateway, mode, nodes.master/backup with optional ssh block) parse strictly; semantic defects are reported together in one error.

## Task Commits

1. Task 1 slugs, four places: `c8800b4`
2. Task 2 `yaml_serde`, `deploy` module, `strict_parse`: `f938cfc`
3. Task 3 `Manifest` / `Target`: `fdf7838`
4. Review fix (coordinator security finding), scrub unknown-variant values: `7bab399`

## libyaml-rs build-script finding

`cargo fetch` then inspection of `~/.cargo/registry/src/*/libyaml-rs-0.3.0`: there is no `build.rs` and Cargo.toml sets `build = false`. `yaml_serde-0.10.7` likewise has `build = false`. Nothing executes at build time. Cargo.lock gained exactly two packages: `yaml_serde` and `libyaml-rs` (`cargo tree -p ignition-core -i libyaml-rs` shows `yaml_serde` as the only dependent). `cargo build -p ignition-cli --no-default-features` passes.

## yaml_serde location convention

1-based line and column. For unknown-field, wrong-type and syntax errors the column is the offending key/token start. For a `custom` error raised inside a value (the `schema_version` check) yaml_serde reports the start of the enclosing mapping, not the key (observed: `a: 1\nb: 2\nschema_version: 2` gives 1:1), so `strict_parse` relocates that one error to the `schema_version:` line at column 1.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] schema_version error position was the mapping start**
- **Found during:** Task 2 (test `wrong_schema_version_is_positioned`)
- **Issue:** yaml_serde positions custom value errors at the first key of the mapping, so a version error in a file whose first key is not `schema_version` pointed at the wrong line.
- **Fix:** `relocate_schema_error` points at the `schema_version:` line, column 1.
- **Files modified:** crates/ignition-core/src/deploy/yamlio.rs
- **Commit:** f938cfc

**2. [Rule 2 - Missing critical functionality] Unknown-variant and enum-tag values echoed in errors**
- **Found during:** coordinator security review of f938cfc
- **Issue:** `scrub_values` only handled `invalid type/value`; serde's `unknown variant `<value>`` echoes the raw value of any enum field, and yaml_serde's nested-enum message echoes a YAML tag.
- **Fix:** `scrub_variant` drops the value and keeps `expected ...` (using the last `, expected ` so a value imitating the message cannot leak a tail); nested-enum message replaced by a static one. Audited the rest of yaml_serde 0.10.7 `error.rs`/`de.rs` and the libyaml error Display: remaining messages are static text plus positions, or name keys (`unknown field`). Added `enum_canary_is_scrubbed_but_variants_are_kept` and an assertion in `target_os_rejects_unknown`.
- **Files modified:** crates/ignition-core/src/deploy/yamlio.rs, crates/ignition-core/src/deploy/target.rs
- **Commit:** 7bab399

**3. [Plan conditional, not triggered] Alias pre-parse guard**
- The plan said to add an alias-token guard if yaml_serde expanded the bomb. It does not expand it (built-in repetition limit), and a 1,024-token count would not catch a 72-alias bomb, so no guard was added. `alias_bomb_is_refused` pins the library behaviour.

**Known residual (accepted):** semantic findings echo declared names (`types[0] "nosep"`, `gateway "../x"`) and `unsupported schema_version N` echoes the integer, as specified. These are identifiers, not credentials.

## Known Stubs

None.

## Threat Flags

None. No new network, auth or file-access surface (parsers take bytes and read no file).

## Verification

- `cargo test -p ignition-core --lib error::tests`: 12 pass
- `cargo test -p ignition-core --lib deploy::`: 24 pass
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo build -p ignition-cli --no-default-features`: clean
- Task 1 commit lists exactly error.rs and README.md

## Self-Check: PASSED
