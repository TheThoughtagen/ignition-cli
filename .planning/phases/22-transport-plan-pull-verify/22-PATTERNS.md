# Phase 22: Transport, Plan, Pull & Verify - Pattern Map

**Mapped:** 2026-10-14
**Files analyzed:** 17 new/modified
**Analogs found:** 15 / 17 (two have only partial analogs)

All paths are under `/Users/pmannion/whiskeyhouse/ignition-cli/crates/` unless stated. Line numbers were read this session.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `ignition-core/src/deploy/transport/process.rs` (ProcessRunner) | utility (mockable subprocess) | request-response | `ignition-core/src/rig/compose.rs` `ComposeRunner`/`DockerCompose` (74-127) | role-match (analog lacks `kill_on_drop`, timeout, stdin handling; new code) |
| `ignition-core/src/deploy/transport/remote.rs` (RemoteShell) | service | request-response | `rig/compose.rs` `DockerCompose::spawn` (argv-only) | partial |
| `ignition-core/src/deploy/transport/mod.rs` (TreeReader/TreeMirror, `safe_remote_path`) | trait/model | file-I/O | `client/deploy_api.rs` `DeployApi` (196) + `deploy/tree.rs` `RelPath` | role-match |
| `ignition-core/src/deploy/transport/sftp.rs` | service | file-I/O | none in-repo (RESEARCH Pattern 3) | no analog |
| `ignition-core/src/deploy/transport/local.rs` | service | file-I/O | `deploy/tree.rs` `walk_out` (359) + `write_out` (438) | role-match |
| `ignition-core/src/deploy/transport/winps.rs` | utility | transform | none (RESEARCH Wave B) | no analog |
| `ignition-core/src/deploy/ledger.rs` | model + strict reader | transform | `deploy/target.rs` + `deploy/yamlio.rs::strict_parse` | role-match |
| `ignition-core/src/deploy/plan.rs` | service (pure) | transform | `deploy/render.rs` + `tree.rs::RenderedTree::render_sha256` | role-match |
| `ignition-core/src/deploy/pull.rs` | service | file-I/O | `deploy/tree.rs::write_out` clobber ladder + `render.rs` | role-match |
| `ignition-core/src/deploy/verify.rs` | service | request-response | `actions/testing.rs` verdict (150-167) | role-match |
| `ignition-core/src/deploy/target.rs` (modify: `known_hosts`, `local{data_dir}`, timeouts) | config/model | transform | itself (`Ssh`, 60-90) | exact |
| `ignition-core/src/actions/deploy.rs` (add `deploy_plan/pull/verify`) | action | request-response | itself `deploy_render` (49-110) | exact |
| `ignition-core/src/error.rs` (new slugs) | model | n/a | `DeployTreeInvalid`/`EmbeddedSecret` four-place rule | exact |
| `ignition-cli/src/cli.rs` (`DeployCommand` Plan/Pull/Verify) | route | request-response | `DeployCommand::Render` (cli.rs 621-640) | exact |
| `ignition-cli/src/main.rs` (ActionOutput, dispatch, verdict hook, GUARDED_OPS) | controller | request-response | `DeployRender` arms (232, 410, 3085-3093); testing hook (536-549); `GUARDED_OPS` (3807) | exact |
| `ignition-tui/src/routes.rs` rows | config | n/a | `deploy render` row (483) | exact |
| Tests (`ignition-cli/tests/contract_deploy.rs`, core `tests/deploy_*.rs`) | test | request-response | `contract_deploy.rs` header (1-40) | exact |

## Pattern Assignments

### `actions/deploy.rs` additions (action, request-response)

**Analog:** `ignition-core/src/actions/deploy.rs` (`deploy_render`)

**Imports** (lines 17-27):
```rust
use std::path::Path;
use serde::Serialize;
use crate::deploy::component_problem;
use crate::deploy::manifest::Manifest;
use crate::deploy::render::{RenderInput, RenderedResource, render};
use crate::deploy::target::Target;
use crate::deploy::tree::{RenderMarker, SourceTree, write_out};
use crate::error::CoreError;
```

**Load-repo preamble to copy verbatim for plan/pull/verify** (lines 55-93):
```rust
if let Some(problem) = component_problem(env) {
    return Err(CoreError::InvalidInput { reason: format!("--env {env}: {problem}") });
}
let target_rel = format!("targets/{env}.yaml");
let target_bytes = read_tree_file(repo, &target_rel, Some(&format!("--env {env} selects {target_rel}")))?;
let target = Target::parse(&target_rel, &target_bytes)?;
// ... require_real_dir(repo, "gateways")?; manifest = Manifest::parse(..)?;
let source = SourceTree::read_gateway(&gateway_dir, env)?;
let outcome = render(&RenderInput { gateway: &target.gateway, env, manifest: &manifest, target: &target, source: &source })?;
```
`read_tree_file`, `require_real_dir`, `tree_meta` (lines 112-183) refuse symlinks; reuse them (make `pub(crate)`) for pull and `--base`. Result structs are `#[derive(Debug, Clone, PartialEq, Serialize)]` with doc comments per key and a module doc listing key order; new keys are additive. Actions never print. Plan/verify differ from render in that they need a `Session::resolve_side` for the master profile (RESEARCH Pattern 8); render is offline (null profile).

### `deploy/transport/process.rs` (utility, request-response)

**Analog:** `rig/compose.rs`

**Trait + mock seam** (lines 74-94): `#[async_trait::async_trait] pub trait ComposeRunner: Send + Sync { async fn run(&self, args: &[String]) -> ComposeOutput; ... }`. Copy the shape (`ProcessRunner::run(program, argv, stdin, timeout) -> ProcessOutput {stdout, stderr, code}`), plus a fake used by unit tests.

**Spawn-failure mapping** (lines 104-127): spawn error becomes code 127 with `failed to spawn ...` stderr, not a panic. Keep that. DIVERGE: the analog uses `.output().await` with no `kill_on_drop`, no timeout, stdin inherited. New code must add `.kill_on_drop(true)`, `stdin(Stdio::piped())` closed immediately, and `tokio::time::timeout` (RESEARCH Pattern 2).

### `deploy/transport/local.rs` and pull write (file-I/O)

**Analog:** `deploy/tree.rs`

**Hardened sorted walk** (lines 357-380): `walk_out(dir, rel, &mut files, &mut dirs) -> Result<(), String>` uses `read_dir`, non-UTF-8 name refusal, `entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()))`, `symlink_metadata`. Promote to `pub(crate)` for LocalFsTransport reads, sftp download-dir walk and `--base` re-hash.

**Hash helper** (line 35): `pub fn sha256_hex(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }`. Use for all client-side hashes and `plan_sha256`.

**Clobber ladder write** (lines 438-470): `check_out` first (refuse before any write), `create_new` for new paths, `write_replacing` (temp file plus rename) for listed paths, never delete. Mirror this for pull; the mirror write half against a scratch dir follows the same create_dir_all/create_new shape. Remember CONTEXT: nothing dot-named or staged inside `data/config/resources/`.

### `deploy/target.rs` (config, modify)

**Analog:** same file. `#[derive(Debug, Clone, PartialEq, Deserialize)] #[serde(deny_unknown_fields)]` on every struct; `Ssh` (lines 60-87) has `#[serde(default)] pub port: Option<u16>`, mutually-exclusive `key_file`/`key_env`, `os: TargetOs`, `data_dir`. Add to `Ssh`: `known_hosts` (required path), `connect_timeout_s`/`op_timeout_s` (`#[serde(default)] Option`). Add to `NodeTarget`: `#[serde(default)] pub local: Option<Local>` with `Local { data_dir }`. Validation lives in `Target::validate` (lines 100-125): collect `DeployFinding` via the local `push` closure, per-node loop over `nodes.master`/`nodes.backup`. Add ssh xor local check there (D-02) and `component_problem`-style checks.

### `deploy/ledger.rs` (strict reader)

**Analog:** `deploy/target.rs::Target::parse` (lines 93-98): `strict_parse(file, bytes)?` then `validate`. Use `yamlio::strict_parse` (size and depth caps, duplicate-key refusal, scrubbed errors) or the `node.rs` JSON hardening noted in RESEARCH Pattern 5. Errors are `CoreError::deploy_tree_invalid(file, line, col, msg)` (error.rs 1519) or a new slug (RESEARCH Open Question 10). D-07: never fall back to empty on corrupt.

### `deploy/plan.rs`, `verify.rs` (pure engines)

**Analog:** `deploy/tree.rs` `RenderedTree::render_sha256` (listing hash; pinned by `RENDER_GOLDEN` in `contract_deploy.rs:25`). Copy the approach of a documented byte-exact canonical text hashed with `sha256_hex`, and pin a golden vector for `plan_sha256`. Use `BTreeMap` throughout for deterministic order.

**Verdict result + exit hook** (analog `actions/testing.rs` 150-167):
```rust
pub fn failure_exit_code(&self) -> Option<u8> {
    match self.verdict.as_deref() { Some("failed") => Some(6), _ => None }
}
```
For verify: `Some(6)` when `!self.ok`. The slug rides `data.slug`; it is not a CoreError and never joins an exit table.

### `ignition-cli/src/cli.rs` (route)

**Analog:** `DeployCommand::Render` (cli.rs ~621-640):
```rust
Render {
    #[arg(long, value_name = "ENV")] env: String,
    #[arg(long, value_name = "DIR", default_value = ".")] repo: PathBuf,
    #[arg(long, value_name = "DIR")] out: Option<PathBuf>,
}
```
Add `Plan{env, repo, base: Option<PathBuf>, diff: bool}`, `Pull{env, repo}` (global `--yes` already exists as `cli.yes`), `Verify{env, repo, project}` (`--project` default `ign-cli`, per other tags verbs).

### `ignition-cli/src/main.rs` (controller)

**ActionOutput variant + render arm** (lines 232, 410): `DeployRender(actions::deploy::DeployRenderResult)` and `ActionOutput::DeployRender(result) => render_success(profile, result, compact)`. Add `DeployPlan`, `DeployPull`, `DeployVerify`.

**Dispatch arm** (lines 3085-3093): 
```rust
Commands::Deploy(DeployArgs { command }) => match command {
    DeployCommand::Render { env, repo, out } => {
        let result = actions::deploy::deploy_render(&repo, &env, out.as_deref()).map(ActionOutput::DeployRender);
        (None, result)
    }
},
```
Plan/verify return the master profile name instead of `None` (resolve from the target, not `--profile`).

**Verdict hook** (lines ~536-549; copy and extend):
```rust
if let ActionOutput::TestingRun(testing) = &out
    && let Some(code) = testing.failure_exit_code()
{
    return ExitCode::from(code);
}
ExitCode::SUCCESS
```
Add a `DeployVerify` branch the same way, after the envelope is rendered.

**Guarded pull** (`GUARDED_OPS`, line 3807; marker precedent at 3077-3081):
```rust
// guarded:e2e init
if let Err(err) = require_confirmation(cli.yes, &preview) { return (profile, Err(err)); }
```
Add `("deploy pull", "<representative refusal prose>")` to `GUARDED_OPS` and an adjacent `// guarded:deploy pull` marker; the drift test (4470-4525) walks both directions. For a dynamic prose site, the prose is a `format!` listing what would be replaced (D-11). Only gate when `gateways/<name>/` is non-empty.

### `ignition-tui/src/routes.rs`

**Analog** (lines 483-486): 
```rust
CliRoute { path: "deploy render", mapping: Mapping::Screen(Screen::Projects) },
```
Add `deploy plan`, `deploy pull`, `deploy verify` rows in the same commit as the clap commands; `deploy_rows_cover_the_family` walks both directions.

### MCP (`ignition-cli/src/mcp.rs`)

**Analog:** `deploy_render_is_an_unguarded_catalog_tool` (line 865): asserts the tool derives from the clap tree, is not in `GUARDED_OPS`, and has no `confirm` property. Copy for `deploy_plan` and `deploy_verify`. For `deploy_pull` assert the opposite (guarded, `confirm` advertised).

### `error.rs` new slugs (four-place rule)

**Analog:** `DeployTreeInvalid`/`EmbeddedSecret`: variant (error.rs ~801), `code()` (1037-1038), `exit_code()` (1051), `hint()` (1462), header exit table (line 11), `DEPLOY_SLUGS` (1516: `&["deploy_tree_invalid", "embedded_secret"]`), README exit table, `EXIT_SLUG_LITERALS` / enumerated test (~2090 `(2, "embedded_secret")`), and the `DEPLOY_SLUGS` agreement test (2846). Read the README and these tests before choosing any exit code; do not restate codes from memory. Slugs likely needed: `transport_failed`, a ledger target-state slug, `plan_stale` is Phase 23 (not here).

### Tests

**Analog:** `ignition-cli/tests/contract_deploy.rs`: `assert_cmd::Command`, `TRACER_REPO = concat!(env!("CARGO_MANIFEST_DIR"), "/../ignition-core/tests/fixtures/deploy/repos/tracer")`, a pinned golden (`RENDER_GOLDEN`), a `DATA_KEYS` list asserting key order, and isolated absent config with the gateway env stripped. For plan/verify binary tests use `wiremock` with an isolated config. Core tests: `tests/deploy_render.rs`, `tests/deploy_client_contract.rs` (stateful fake `DeployApi`, wrap in `ForbidWrites`). Live tests follow `tests/live_deploy_api.rs` (`#[ignore]`).

## Shared Patterns

### Actions never print; envelope is the only stdout
**Source:** `actions/deploy.rs` module doc; CONTEXT "Established Patterns". Progress goes to stderr via a sink.

### Value-free errors
**Source:** `actions/deploy.rs` module doc ("Errors never echo file contents"), `lint::scan_embedded`/`scan_secrets`. Findings name paths and keys only; apply to plan `--diff`, verify, pull `skipped[]`, and ssh stderr (scrub and cap).

### Symlink-refusing filesystem access
**Source:** `actions/deploy.rs::tree_meta/read_tree_file` (112-183) and `tree.rs::walk_out`. Apply to known_hosts copy, `--base`, pull output and local transport.

### Strict serde parsing
**Source:** `target.rs` `deny_unknown_fields` plus `yamlio::strict_parse`. Apply to ledger, target additions, generated manifest read-back.

### Surface wiring in the same commit series
**Source:** `routes.rs` rows, `GUARDED_OPS` + markers, README Commands rows, exit-table rows, `DEPLOY_SLUGS`, mcp catalog tests.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `transport/sftp.rs` | service | file-I/O | No ssh/sftp code exists; use RESEARCH Pattern 2/3 (hardened argv, existence-first batches, `safe_remote_path`) |
| `transport/winps.rs` | utility | transform | No PowerShell/EncodedCommand code; gated on C7a (D-14) |
| `similar` diff for `plan --diff` | utility | transform | Promote `similar = "3"` to core deps (already in Cargo.lock) |

## Metadata

**Analog search scope:** `ignition-core/src/{actions,deploy,rig,client}`, `ignition-cli/src/{main,cli,mcp}.rs`, `ignition-tui/src/routes.rs`, `ignition-cli/tests`
**Pattern extraction date:** 2026-10-14
**Limitation:** RESEARCH.md was read to line 435 of 603 (budget cap); the capture procedure section at the tail was not read.
