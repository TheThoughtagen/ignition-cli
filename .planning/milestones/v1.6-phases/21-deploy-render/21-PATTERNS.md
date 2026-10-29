# Phase 21: Deploy Render - Pattern Map

**Mapped:** 2026-10-12
**Files analyzed:** 27 new/modified
**Analogs found:** 17 / 27 (the pure-logic `deploy/*` modules have no codebase analog; RESEARCH.md Patterns 1-7 are the source)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/ignition-core/src/error.rs` (edit: `DeployTreeInvalid`, `EmbeddedSecret`) | model (error enum) | request-response | same file, `InvalidInput` / `InvalidImportFile` | exact (four-place rule) |
| `README.md` (edit: exit row 2, Commands row, `### Deploy - render`) | config/docs | n/a | `### Workspace` section (~line 1343), exit table (~105-114) | exact |
| `crates/ignition-core/src/lib.rs` (edit: `pub mod deploy;`) | config | n/a | existing `pub mod rig;` / `module;` / `webdev;` lines 24-34 | exact |
| `crates/ignition-core/src/actions/mod.rs` (edit: `pub mod deploy;`) | config | n/a | `pub mod workspace;` line 35 | exact |
| `crates/ignition-core/src/actions/deploy.rs` | service (action) | file-I/O, transform | `actions/workspace.rs` (clobber ladder, manifest read) and `actions/lint.rs` (local-only, no Session) | role-match |
| `crates/ignition-core/src/deploy/{mod,manifest,target,yamlio}.rs` | model/utility | transform | `workspace.rs` `WorkspaceManifest` (schema_version + serde) | partial |
| `crates/ignition-core/src/deploy/{node,layout,rules,uuid,lint,tree,render}.rs` | utility | transform | `tests/workspace_path_mapping.rs` + `client/workspace.rs` `build_mapping` (single injective path type, fail-closed) | partial |
| `crates/ignition-cli/src/cli.rs` (edit: `Commands::Deploy`, `DeployArgs`, `DeployCommand::Render`) | route (clap) | request-response | `WorkspaceArgs`/`WorkspaceCommand` (lines 188, 539-600) | exact |
| `crates/ignition-cli/src/main.rs` (edit: `ActionOutput::DeployRender`, 2 match arms, dispatch) | controller | request-response | `Commands::Lint` arm (2871-2880), `ActionOutput::Workspace*` (301-307, 426-428) | exact |
| `crates/ignition-cli/src/render.rs` (edit: human render arm) | component | transform | `ActionOutput::Workspace*` arms (351-353) | exact |
| `crates/ignition-tui/src/routes.rs` (edit: `deploy render` row) | config (registry) | n/a | `workspace checkout` row (~433-445) | exact |
| `crates/ignition-core/tests/deploy_corpus.rs` | test | batch | `tests/workspace_path_mapping.rs` (proptest + fixtures) | role-match |
| `crates/ignition-core/tests/deploy_render.rs` | test | file-I/O | `tests/workspace_checkout.rs` | role-match |
| `crates/ignition-cli/tests/contract_deploy.rs` | test | request-response | `tests/contract_workspace.rs` (harness lines 1-50), `contract_lint.rs` for no-gateway form | role-match |
| `crates/ignition-cli/tests/deploy_family_coverage.rs` | test | transform | `tests/tui_coverage.rs` (clap-tree walk) | role-match |
| `crates/ignition-cli/tests/contract_stdout_purity.rs` (edit) | test | request-response | itself | exact |
| `.gitattributes` (edit) | config | n/a | existing `crates/ignition-core/tests/fixtures/*.xml -text` | exact |
| `crates/ignition-core/tests/fixtures/deploy/**` (corpus, SANITIZE.md, openapi extract) | fixture | n/a | `fixtures/live-rig-deploy/`, `20-openapi-extract.json` | partial |
| `Cargo.toml` / `crates/ignition-core/Cargo.toml` (edit: `yaml_serde`) | config | n/a | existing `sha2`/`serde_json` workspace entries | exact |

## Pattern Assignments

### `crates/ignition-core/src/error.rs` (error enum, four-place rule)

**Analog:** same file, `InvalidInput` (variant 130-137, `code()` 866, `exit_code()` 915-921, `hint()` ~992).

**Variant shape** (lines ~130-137):
```rust
    /// A command input the caller must fix ... Exit 2 — usage class
    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },
```
New variants follow it: `DeployTreeInvalid { file, line: Option<usize>, column: Option<usize>, message }` and `EmbeddedSecret { paths: Vec<String> }` (paths only, never values). Box the payload if it exceeds two Strings (`clippy::result_large_err`, WINDOWS.md entry 3).

**code() / exit_code()** (866, 915-921): add arms `Self::DeployTreeInvalid{..} => "deploy_tree_invalid"`, `Self::EmbeddedSecret{..} => "embedded_secret"`, and add both to the `=> 2` or-pattern beside `InvalidInput`.

**Four places, ONE commit:**
1. module-header exit table in `error.rs` (exit-2 row)
2. variants + `code()` + `exit_code()` + `hint()` + `endpoint()`
3. `exit_code_mapping_enumerated` (line 1484) case and `EXIT_SLUG_LITERALS` (line 1866):
```rust
    const EXIT_SLUG_LITERALS: &[(u8, &str)] = &[
        (2, "invalid_input"),
        (2, "gateway_client_error"),
        // add: (2, "deploy_tree_invalid"), (2, "embedded_secret"),
```
4. `README.md` exit table row 2 (line ~112), slug column: `` `confirmation_required`, `invalid_import_file`, `invalid_input`, `gateway_client_error` (P9) `` gains the two slugs. `readme_exit_table_agreement` (~2025-2060) parses both directions.
Also expose `CoreError::DEPLOY_SLUGS` const for the family coverage test. Hints must name only real fixes (no invented command).

---

### `crates/ignition-core/src/actions/deploy.rs` (action, file-I/O)

**Analogs:** `actions/workspace.rs` for the clobber ladder (doc ~205: "a non-empty target that is not a valid workspace ... never clobbers an unrelated directory"; test `recheckout_gate_refuses_unrelated_non_empty_targets`, line 1196) and `actions/lint.rs` for a local-only action with no `GatewayApi`.

**Imports pattern** (workspace.rs 25-40):
```rust
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::CoreError;
```
No `GatewayApi`, no `Session` (render is offline).

**Manifest/schema pattern** (workspace.rs 36-50): `pub const ..._SCHEMA_VERSION: u8 = 1;`, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` struct with `schema_version` first; envelope discipline: serde field order = struct order, future optional fields `#[serde(skip_serializing_if = "Option::is_none")]`. Result model `DeployRenderResult { env, gateway, mode, out, files, bytes, render_sha256, resources, tag_providers, uuids_minted, opaque_files, warnings }` (all keys always present).

**--out ladder:** copy the workspace refusal (absent: create; empty or only a prior `render.json` marker: ok; else `CoreError::InvalidInput`), regular files only, no symlinks, never delete.

---

### `crates/ignition-core/src/deploy/*.rs` (pure transform modules)

No codebase analog for `node.rs` (Gson writer), `yamlio.rs`, `rules.rs`, `uuid.rs`, `lint.rs`: use RESEARCH.md Pattern 1 (RawValue `Node`), Pattern 2 + "Strict parse wrapper" (yaml_serde schema-first two-pass), Pattern 4 (rule table, sha2 v8 uuid), Pattern 5 (`render_sha256`), Pattern 6 (collect-all lints), Pattern 7 (secret-path table).

Partial analog for `layout.rs` `RelPath`: `crates/ignition-core/src/client/workspace.rs::build_mapping` plus `tests/workspace_path_mapping.rs` (one injective mapping computed once, fail closed, proptest on hostile names). Do NOT reuse `MemberSource::Tree` or the FNV-1a mapping (RESEARCH anti-pattern).

Module registration: `lib.rs` lines 24-34 list `pub mod` alphabetically (`config; e2e; error; ...`); insert `pub mod deploy;` after `config`.

---

### `crates/ignition-cli/src/cli.rs` (clap)

**Analog:** `Workspace` at lines 183-188 and `WorkspaceArgs`/`WorkspaceCommand` at 539-600.

**Pattern:**
```rust
    #[command(arg_required_else_help = true)]
    Workspace(WorkspaceArgs),
...
#[derive(Debug, clap::Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceCommand,
}
#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    Status {
        /// Workspace root (default: the current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
```
Add `Deploy(DeployArgs)` with `DeployCommand::Render { #[arg(long)] env: String, #[arg(long, default_value = ".")] repo: PathBuf, #[arg(long)] out: Option<PathBuf> }`. `deploy` is a required-subcommand group (non-Option `command`), so only the leaf needs a TUI row. Doc comments become MCP tool descriptions (catalog derived via `Cli::command()`); no `confirm`, no `GUARDED_OPS`.

---

### `crates/ignition-cli/src/main.rs` (dispatch + ActionOutput)

**Analog (local-only dispatch, profile null):** lines 2871-2880:
```rust
        Commands::Lint(LintArgs { paths, strict, passthrough }) => {
            let result = actions::lint::lint_run(&paths, strict, &passthrough)
                .await
                .map(ActionOutput::Lint);
            (None, result)
        }
```
Copy as `Commands::Deploy(DeployArgs { command: DeployCommand::Render { env, repo, out } }) => { ... (None, result) }`. No `Session::resolve`, no `// guarded:` marker.

**ActionOutput variant** (lines 296-307) with a doc comment naming the envelope keys:
```rust
    WorkspaceStatus(actions::workspace::WorkspaceStatus),
```
Add `DeployRender(actions::deploy::DeployRenderResult)`.

**JSON arm** (line ~426): `ActionOutput::DeployRender(result) => render_success(profile, result, compact),`. Also update the human arm in `render.rs` (~351-353, `ActionOutput::WorkspaceStatus(result) => render_workspace_status_human(result)`). Both matches are exhaustive: compile fails if one is missed.

---

### `crates/ignition-tui/src/routes.rs` (registry row)

**Analog** (~433-445):
```rust
        CliRoute {
            path: "workspace checkout",
            mapping: Mapping::Screen(Screen::Projects),
        },
```
Add `CliRoute { path: "deploy render", mapping: Mapping::Screen(Screen::Projects) }` in the SAME change as the clap leaf (`tui_coverage.rs` walks both directions). Do not grow the pinned OutOfBand set (`out_of_band_rows_are_pinned`) or move the Dashboard pin (41).

---

### `crates/ignition-cli/tests/deploy_family_coverage.rs` (test)

**Analog:** `tests/tui_coverage.rs` (header 1-60).
```rust
#![cfg(feature = "tui")]
use clap::CommandFactory;
use ignition_cli::cli::Cli;
use ignition_tui::routes::{Mapping, menu_label, routes};
```
Walk `Cli::command()` to the `deploy` subcommand, take leaves (skip clap's `help`). Assert (a) routes row `deploy <leaf>` on `Screen::Projects` (the only `#[cfg(feature = "tui")]` part, so gate it per test, not file-wide, to let README/clap parts run with `--no-default-features`), (b) README Commands row via `include_str!("../../../README.md")` containing `` `ign deploy <leaf>`` , (c) every `CoreError::DEPLOY_SLUGS` entry in the README exit table.

---

### `crates/ignition-cli/tests/contract_deploy.rs` (test)

**Analog:** `tests/contract_workspace.rs` harness (lines 23-50): isolated config via `tempfile`, `assert_cmd::Command::cargo_bin("ign")`, snapbox goldens. Render needs no wiremock and no profile, so drop the `IGNITION_URL/TOKEN` env lines; set `IGNITION_CLI_CONFIG` to an isolated path anyway. Tests: `render_digest_golden`, `embedded_secret_names_path_not_value` (canary never in stdout/stderr).

---

### `crates/ignition-core/tests/deploy_corpus.rs` / `deploy_render.rs`

**Analogs:** `tests/workspace_path_mapping.rs` (proptest, hostile names) and `tests/workspace_checkout.rs` (tempdir trees, clobber ladder). Run on all three CI OSes; no shell-outs, no `/` assumptions. Fixture must contain no `\r` (assert it).

---

### `.gitattributes`

**Analog:**
```
crates/ignition-core/tests/fixtures/*.xml -text
```
Add `crates/ignition-core/tests/fixtures/deploy/** -text` and `*.jar binary`.

---

## Shared Patterns

### Four-place slug rule
**Source:** `error.rs` (header table, `code()`/`exit_code()`, `EXIT_SLUG_LITERALS` 1866, `exit_code_mapping_enumerated` 1484) + `README.md` row 2. **Apply to:** `deploy_tree_invalid`, `embedded_secret`; one commit (`feat(21-NN): deploy_tree_invalid and embedded_secret slugs (four-place rule)`).

### Local-only verb (profile null)
**Source:** `main.rs` 2871-2880 (`Commands::Lint`). **Apply to:** `deploy render` dispatch. Errors return `(None, Err(..))`; `error_profile` (3428) only echoes for `SecretUnavailable`.

### Command-family atomicity
**Source:** routes.rs comment "Rows land in the SAME task as their clap commands (Pitfall 5)". **Apply to:** clap leaf + routes row + README row + family test together.

### Envelope discipline
**Source:** `workspace.rs` module doc. Struct-order serde, additive keys, `render_success(profile, result, compact)` for the envelope.

### Secrets never echoed
`EmbeddedSecret` carries JSON paths only; scrub yaml_serde messages that echo values (`invalid type: string "..."`); canary test over Display, envelope, and stderr.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `deploy/node.rs` | utility | transform | No order-preserving JSON writer exists; use RESEARCH Pattern 1 (prototyped 178/183) |
| `deploy/yamlio.rs`, `manifest.rs`, `target.rs` | utility/model | transform | No YAML in codebase yet (`yaml_serde` is new); use RESEARCH Pattern 2 and schema skeletons |
| `deploy/rules.rs`, `uuid.rs`, `lint.rs` | utility | transform | New domain tables; RESEARCH Patterns 4, 6, 7 |
| `deploy/tree.rs` (`render_sha256`) | utility | transform | `sha2` exists as a dep, but no existing listing-digest code; RESEARCH Pattern 5 |
| sanitized corpus + `SANITIZE.md` | fixture | n/a | Public-repo leak risk (RESEARCH Pitfall 1); scripts via `uv run` / `python3 -I` |
| `21-LIVE-CAPTURES.md` + `capture.sh` subcommands | doc/script | n/a | Extend `tests/fixtures/live-rig-deploy/capture.sh` (Phase 20 precedent) |

## Metadata

**Analog search scope:** `crates/ignition-core/src`, `crates/ignition-core/tests`, `crates/ignition-cli/{src,tests}`, `crates/ignition-tui/src`, `README.md`, `.gitattributes`
**Files scanned:** about 20 read or grepped (targeted ranges)
**Pattern extraction date:** 2026-10-12
