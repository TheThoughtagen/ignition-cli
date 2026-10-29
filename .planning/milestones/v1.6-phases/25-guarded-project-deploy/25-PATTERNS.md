# Phase 25: Guarded Project Deploy - Pattern Map

**Mapped:** 2026-10-22
**Files analyzed:** 16 new/modified
**Analogs found:** 15 / 16 (host-probe write half has a partial analog only)

All paths relative to `/Users/pmannion/whiskeyhouse/ignition-cli/crates/` unless stated. Line numbers verified this session.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `ignition-core/src/deploy/projects.rs` (NEW: `ProjectFile`, artifact build, canonical digest, parent graph, release compare, `ReleaseRecord`) | model/utility | transform | `ignition-core/src/deploy/manifest.rs` (+ `deploy/ledger.rs` for record parse) | role-match |
| `ignition-core/src/deploy/transport/projects.rs` (NEW: `ProjectHost` git probe, release record read/write) | service | file-I/O | `TreeReader::read_ledger` in `deploy/transport/mod.rs:138-167` + `deploy/ledger.rs` | role-match |
| `ignition-core/src/actions/deploy_projects.rs` (NEW: prepare/run, children sweep) | service/action | request-response + CRUD | `ignition-core/src/actions/deploy_apply.rs` (`deploy_apply_prepare`/`_run`) | exact |
| `ignition-core/src/actions/projects.rs` (MOD: `validate_import` to `pub(crate)`) | utility | transform | itself, line 144 | exact |
| `ignition-core/src/actions/deploy.rs` (MOD: `load_target` accepts a targets dir) | service | file-I/O | itself, lines 144-206 | exact |
| `ignition-core/src/deploy/target.rs` (MOD: reserve mode `projects`; known_hosts resolve vs targets dir) | model | transform | itself, `validate` reserved-mode check | exact |
| `ignition-core/src/error.rs` (MOD: `ProjectHashMismatch` + 2 more slugs) | model | n/a | `PlanStale` (error.rs:846, code at 1275, exit class at 1343, hint at 1746) | exact |
| `ignition-core/src/deploy/apply/preflight.rs` (MOD: extract designers loop) | utility | polling | itself (`designers_finding`, `DesignersPolicy`) | exact |
| `ignition-cli/src/cli.rs` (MOD: `DeployCommand::Projects` leaf) | config/CLI | request-response | `DeployCommand::Apply` (cli.rs:736-775+) | exact |
| `ignition-cli/src/main.rs` (MOD: dispatch + `GUARDED_OPS`) | controller | request-response | `DeployCommand::Apply` arm (main.rs:3331-3492); `GUARDED_OPS` at 4354 | exact |
| `ignition-cli/src/render.rs` (MOD: human render + exit-6 hook) | utility | transform | existing `DeployApply` renderer + `failure_exit_code()` hook (main.rs ~601) | exact |
| `ignition-cli/src/mcp.rs` (MOD: `EXCLUDED_PATHS`) | config | n/a | lines 72, 880, 935, 960 | exact |
| `ignition-tui/src/routes.rs` (MOD: one `CliRoute` row) | config | n/a | routes.rs:478-505 | exact |
| `ignition-core/tests/deploy_projects.rs` (NEW) | test | request-response | `tests/deploy_apply.rs` + `tests/common/{fake_deploy,forbid_writes,deploy_scenario}.rs` | exact |
| `ignition-core/tests/common/fake_deploy.rs` (MOD: project store) | test helper | CRUD | itself | exact |
| `ignition-core/tests/live_deploy_projects.rs` (NEW, ignored) | test | request-response | `tests/live_deploy_apply.rs` | exact |
| `README.md`, `.github/workflows/deploy-projects.yml` (optional) | docs/CI | n/a | README "Running deploy in CI"; `.github/workflows/deploy-apply.yml` | exact |

## Pattern Assignments

### `ignition-core/src/deploy/projects.rs` (model/utility, transform)

**Analog:** `ignition-core/src/deploy/manifest.rs`

**Imports + strict struct pattern** (manifest.rs:1-52):
```rust
use serde::Deserialize;
use super::component_problem;
use super::yamlio::{SchemaVersion, strict_parse};
use crate::error::{CoreError, DeployFinding};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: SchemaVersion,
    ...
}
```
Apply to `ProjectFile { schema_version, project, environments }`. Parse with `strict_parse::<T>(file, bytes)` (`yamlio.rs:60`); validate `project`/each env with `component_problem` and collect semantic defects into ONE `deploy_tree_invalid` (manifest.rs module doc). Leave `Manifest.projects`/`ProjectPin` (manifest.rs:40-52) untouched and unused (D-04).

**Canonical digest:** reuse `resources::normalize_descriptor` (`client/resources.rs:455`) and `deploy::tree::sha256_hex`. Do NOT use `MemberSource::members()/member_hashes()` (returns empty on checkout layout, drops `project.json`, FNV).

**Release record parse:** copy the `Ledger::parse` pattern in `deploy/ledger.rs` (strict JSON, `schema_version: 1`, `deny_unknown_fields`, size cap, any anomaly typed refusal, never "absent"). Release order: `semver::Version::parse(tag.trim_start_matches('v'))`.

**Zip validation before upload:** `validate_import` (actions/projects.rs:144-181):
```rust
fn validate_import(zip: &[u8]) -> Result<(), CoreError> {
    if !zip.starts_with(&ZIP_MAGIC) { return Err(CoreError::InvalidImportFile { reason: ... }); }
    if let Some(err) = import_size_error(zip.len()) { return Err(err); }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(...)?;
    for index in 0..archive.len() { /* by_index + read_to_end: catches truncation */ }
    Ok(())
}
```
Change `fn` to `pub(crate) fn`; call on the built artifact. Also refuse empty adoptable member set locally and an import whose `changes` is empty (Pitfall 1).

---

### `ignition-core/src/deploy/transport/projects.rs` (service, file-I/O)

**Analog:** `TreeReader` in `deploy/transport/mod.rs:138-167` (`read_ledger`/`read_journal`: `Option<Vec<u8>>`, "a failed read is never an empty read", symlink/non-regular/over-cap = typed error) and `deploy/ledger.rs` (`ledger_rel = format!("ign-deploy/{gateway}/{mode}/{LEDGER_FILE}")`, `LedgerRoot::for_node`: account home for ssh, data dir for local).

Copy: node access via `node_access`/`for_node` (error text "set ssh or local; this verb needs host access"); local impl in `transport/local*.rs`, ssh in `sftp.rs`, Windows in `winps.rs` (use `Test-Path -LiteralPath -Force`). Put the new write on a SEPARATE trait; extend the `write_half_unreachable` source-scan test in `transport/mod.rs` (do not widen `TreeReader`). Record write is temp-then-rename. Record path: `ign-deploy/<gateway>/projects/<project>/release.json`; add `projects` to the reserved-mode refusal in `Target::validate` (next to `core`, `local`).

Conformance tests: copy `tests/deploy_host_conformance.rs` and `tests/common/faulty_host.rs`.

---

### `ignition-core/src/actions/deploy_projects.rs` (action, request-response)

**Analog:** `ignition-core/src/actions/deploy_apply.rs` (`deploy_apply_prepare` returning `ApplyPrepared::{Ready{prose,..},Noop(result)}`, `deploy_apply_run`, `ApplySurfaces`, `ApplyRunOptions`). Mirror: `Prepared::{Ready, Noop}` enum, `--check` returns the prepared result with zero writes, run does preflights, `RunDir::create_fresh(repo, gateway)` (run_store.rs:200, git-ignore guard, 0700/0600), `run_dir.write_json("run.json"/"plan.json", &v)` (run_store.rs:355, write-once), `EventLog`, pre-image export (`project_export_to_file`) into the run dir, `project_import(name, zip, overwrite)`, post-import export + digest compare, verdict slug exit 6 after envelope via `failure_exit_code()` (see `DeployVerifyResult::failure_exit_code`, actions/deploy.rs:917). Actions never print. Result struct derives Serialize like `DeployPlanResult` (actions/deploy.rs:241).

Gateway reads: `projects()`, `project_find`, `designers()`, `actions::resources::export_zip_bytes` (actions/resources.rs:150). Parent absent / `.git` present / downgrade / first-deploy-change refusals: `CoreError::DestructiveChangeRefused { gate, keys, count }` (exit 2, free-string gate) for flag-needed cases; `ProjectHashMismatch` for D-01.

---

### `ignition-core/src/actions/deploy.rs` (MOD: targets dir)

**Analog:** itself (lines 144-158):
```rust
pub fn load_target(repo: &Path, env: &str) -> Result<(String, Target), CoreError> {
    if let Some(problem) = component_problem(env) {
        return Err(CoreError::InvalidInput { reason: format!("--env {env}: {problem}") });
    }
    let target_rel = format!("targets/{env}.yaml");
    let target_bytes = read_repo_file(repo, &target_rel, Some(&format!("--env {env} selects {target_rel}")))?;
    let target = Target::parse(&target_rel, &target_bytes)?;
    Ok((target_rel, target))
}
```
Add a sibling taking `targets_root: &Path` (default `repo`); `known_hosts` paths resolve against it. `transport::for_node(&repo, &declared.target_file, "nodes.master", master)` (main.rs:3366) takes the repo root: pass the targets root there.

---

### `ignition-core/src/error.rs` (MOD)

**Analog:** `PlanStale` (error.rs:846-855). Four-place rule: variant (with doc + `#[error]`), `code()` arm (line ~1275 `=> "plan_stale"`), exit-class arm (line ~1343, target_state exit 6), `hint()` arm (~1746). Add `ProjectHashMismatch { project, expected, actual }` (hashes only), plus slugs for unverified-post-import verdict and `.git`/parent refusals as decided. Update the README exit table and the `readme_exit_table_agreement` / `exit_code_mapping_enumerated` tests.

---

### `ignition-cli/src/cli.rs` and `main.rs` (controller, request-response)

**Analog:** `DeployCommand::Apply` (cli.rs:736+; main.rs:3331-3492).

Decision C block (main.rs:3343-3363), adapted (skip when `--check`):
```rust
if plan_sha256.is_none()
    && (cli.json || cli.compact
        || !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
{
    return (None, Err(CoreError::InvalidInput { reason: "deploy apply needs --plan-sha256 when ..." .to_string() }));
}
```
Guard site (main.rs:3395-3398), keep the marker comment exactly:
```rust
if let actions::deploy_apply::ApplyPrepared::Ready { prose, .. } = &prepared {
    // guarded:deploy apply
    if let Err(err) = require_confirmation(cli.yes, prose) { return (profile, Err(err)); }
}
```
New marker: `// guarded:deploy projects`. Designers flags: copy `--designers`/`--designers-timeout` clap args (`DesignersArg`) and the `DesignersPolicy` construction (main.rs:3401-3415). Per-profile session: `named_profile_client(&mut config, &master.profile)` (main.rs:3419). No-op/check returns before any session (main.rs:3416-3418 precedent). Observer: `render::StderrApplyObserver` in Human mode.

`GUARDED_OPS` (main.rs:4354) add `("deploy projects", "deploy projects overwrite-imports the project into the gateway from the repo checkout")`; the drift test (main.rs ~5047) pins markers both directions.

Flag names (recommended in research, confirm): `--targets DIR`, `--release TAG`, `--artifact-sha256 HASH`, `--check`, `--allow-downgrade`, first-deploy override flag.

---

### `ignition-cli/src/mcp.rs` (config)

mcp.rs:72 `EXCLUDED_PATHS = &["deploy apply", "deploy rollback"]` -> add `"deploy projects"`; line 880 `GUARDED_AND_EXCLUDED` likewise; update `deploy_rollback_is_excluded_by_full_path` set equality (~960) and add `deploy_projects_is_excluded_by_full_path` next to the test at ~935.

### `ignition-tui/src/routes.rs`

Add after the `deploy rollback` row (routes.rs ~503):
```rust
CliRoute { path: "deploy projects", mapping: Mapping::Screen(Screen::Projects) },
```
Same task as the clap leaf (comment at routes.rs:478-482, Pitfall 5).

---

### Tests

- `tests/deploy_projects.rs`: copy structure of `tests/deploy_apply.rs`; fakes from `tests/common/fake_deploy.rs` (extend with project store: list/find/export/import, stamped `lastModification`, overwrite semantics), `tests/common/faulty_deploy.rs`, `faulty_host.rs`.
- `--check` zero-write proof: copy `tests/common/forbid_writes.rs` (`ForbidWrites<T>`: forwards reads, `refuse(method)` panics on writes). A project-store equivalent is needed (the existing one wraps `DeployApi` only).
- Live gate: `tests/live_deploy_projects.rs` modeled on `tests/live_deploy_apply.rs` (`#[ignore]`).
- Capture-first plan 01 records C8 (incl. layout C8c, stray members C8d), C9, C10 with a blocking contract checkpoint; precedent is the 24-02 plan.

## Shared Patterns

### Gate order
**Source:** main.rs:3331-3492. Decision C before any host/gateway contact -> read-only prepare -> `require_confirmation` with plan prose -> designers policy -> noop returns before sessions -> run.

### Run store
**Source:** `deploy/apply/run_store.rs` (`RUN_DIR_ROOT = ".ign/deploy"`, `RunDir::create_fresh`, `write_json`, `create_private_file`). Exclude `.ign` at every depth from the artifact walk (pin in a test).

### Fail-closed host reads
**Source:** `TreeReader::read_ledger` doc, mod.rs:147-155. Absent = no level of path exists; any anomaly is a typed error.

### Strict file parsing
**Source:** `deploy/yamlio.rs:33,60` (`SchemaVersion`, `strict_parse`; CR refused). `.ignexclude` tolerates CRLF (plain path list).

### Error slugs
**Source:** `PlanStale` four-place wiring in `error.rs`; hashes and names only in messages, secret canary applies to output and run files.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| Deterministic artifact zip builder + `.ignexclude` matcher (inside `deploy/projects.rs`) | utility | file-I/O | Only zip-rewrite precedent is `rewrite_options()` in `client/resources.rs` (deflate, folder entries); there is no directory-to-zip builder and no glob crate in the tree (hand-roll tiny grammar per RESEARCH). Use RESEARCH Pattern 3. |
| Release record WRITE over ssh/local | service | file-I/O | Existing transports write only staged config trees; the account-home write precedent is the ledger/journal under `ign-deploy/` (check `deploy/apply/host.rs` and `ledger.rs` for how the ledger is written) but no per-project record exists. |

## Metadata

**Analog search scope:** `crates/ignition-core/src/{actions,deploy,client,error.rs}`, `crates/ignition-cli/src/{cli,main,mcp}.rs`, `crates/ignition-tui/src/routes.rs`, `crates/ignition-core/tests`
**Files scanned:** ~20 read or grepped
**Pattern extraction date:** 2026-10-22
