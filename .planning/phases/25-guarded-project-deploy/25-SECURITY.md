---
phase: 25
slug: guarded-project-deploy
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
block_on: high
created: 2026-10-29
---

# Phase 25 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

The register comes from the `<threat_model>` blocks of plans 25-01 to 25-27: 106 rows and 100 unique threats. T-25-40, T-25-42 and T-25-46 are each listed in three plans (25-13 or 25-14, 25-20, 25-21) and are scored at their highest listed severity. Two unregistered flags from the plan summaries are added as T-25-F1 and T-25-F2.

---

## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| `.ignexclude` → walk | A repo-controlled pattern file |
| ACL grant → capture hosts | A wider grant than needed weakens least privilege |
| action → host record | An on-box file other runners will trust |
| adopt result → keyring / one-time print | Minted key material |
| apply run directory → local disk | The run directory holds records and the saved pre-image of a project |
| approved apply job (environment) → write secrets | Environment secrets released after approval |
| C10 gateway → git module | A module that pulls from git and writes projects |
| caller repository → this repository's actions (cross-org `uses:`) | The caller runs step logic from a pinned ref of a public repository |
| caller runner pool → deploy job | Any runner matching runs-on can take the job and its secrets |
| capture driver → gateway | Shell script sends writes (imports, deletes) to a gateway |
| capture evidence → test double | A fake that behaves unlike the gateway lets the engine pass on behaviour the gateway does not have |
| capture recommendation → locked decision | A recommendation could silently override a user decision |
| child records → warnings | Host records written by other runs |
| CI check job (unapproved) → gateway API | The check token is used before any reviewer approves; the gateway's write permission decides what it can do |
| CI or operator → write path | The guard, Decision C and the hash bind a write to a reviewed artifact |
| CI secret (key text) → key file on the runner disk | The private key lands in a temp file that other local accounts and services must not read |
| CLI → gateway | GET requests carrying the project name |
| CLI → MCP catalog | A verb exposed over MCP is callable by agents without the CLI guard |
| CLI flags → gates | Flags that switch a refusal off |
| CLI output → logs | Human output and progress lines land in CI logs |
| CLI result → CI and agents | Callers decide "deployed or not" from the envelope and the exit code |
| gateway → run directory | The live project's export (secret-bearing) is written to disk |
| gateway parent chain → loop | Gateway data could contain a cycle |
| gateway project list → sweep scope | Gateway data decides which projects are read |
| gateway state → write decision | Open Designer sessions can overwrite or be overwritten |
| GitHub-hosted runner image → release job | vswhere, dumpbin and the MSVC toolchain come from the windows-latest image |
| IaC checkout → ssh trust and host paths | Targets and known_hosts now come from a second checkout |
| job logs and summary → readers | Output visible to everyone with repo read access |
| live tests → gateway | Real imports and deletes against a gateway |
| on-box release record → deploy decisions | A host file another runner wrote decides the downgrade and first-deploy gates |
| operator workstation → capture host (ssh) | The live run reaches a VM whose IP changes on every start, under a pinned host key |
| operator workstation → dev gateway (adopt), → GitHub secrets | Minted keys move from the keychain into GitHub secrets |
| operator workstation (admin OIDC session) → gateway security config | adopt rewrites the whole security-properties singleton |
| project CI → IaC repo | A token crosses into a second repository |
| project content → logs, CI output, run files | Scripts and views can embed credentials |
| project name → filesystem path | A name from `deploy/project.yaml` becomes a path component |
| public repository (this repo, its .planning) ↔ private UAT and infra details | The UAT callers and captures are committed to a public repository |
| README → operators and agents | The documented check is the contract callers script against |
| README template → callers' workflows | Operators copy the template; a wrong template puts write secrets in an unapproved job |
| record file → later runners | Trusted input to the downgrade, first-deploy and children decisions |
| record file → later runners | Trusted input to the downgrade, first-deploy and children decisions; a missing read is read as "never deployed" |
| record writer → host filesystem (replace) | The one host write; a concurrent reader crosses it during a replace |
| record writer → host filesystem (replace) | The one host write; a concurrent reader or a holder without delete-share crosses it |
| release build → GitHub release assets | The binary every deploy runner downloads, checks against its published sha256 and runs with the deploy secrets in its environment |
| repo `project.json` → gateway reads | A repo-controlled parent name drives REST lookups |
| repo checkout → artifact | Untrusted names, sizes and file kinds |
| repo checkout → artifact | Untrusted repo content (paths, symlinks, file sizes) becomes a zip |
| reusable workflow → its own actions checkout | The workflow must run the actions of its own commit, not whatever a branch holds later |
| runbook → administrators | Human steps on production gateways |
| runner → capture hosts (ssh) | Probes and test setup on shared Linux and Windows hosts |
| runner → gateway host filesystem | The host probe reads below the data directory |
| runner → host filesystem (write) | The only host write of the phase |
| runner → Linux and Windows hosts (ssh) | Listings and PowerShell output are host-controlled input |
| runner git config → checkout bytes | System gitconfig can rewrite line endings |
| tag push → production deploy | A pushed tag starts a pipeline that writes to gateways |
| target file → gateway identity | The target names which gateway a write may go to |
| targets checkout → ssh options | `known_hosts` paths come from the IaC repo |
| temporary pre-release (route a) → anyone downloading releases | A non-release build is publicly downloadable for the duration |
| test setup → host filesystem | Creating `.git` and records under the rig's bind root |
| UAT run logs ↔ secrets | Tokens and keys must stay masked |
| unapproved check job → secrets | Runs on tag push before any reviewer; may hold only read-only credentials |
| user paste → captures file | Production configuration could carry credentials |
| WHK-Global checkout → captures file | Real project content could leak into a committed file |
| whoami.exe output → icacls arguments | Text from a child process becomes part of an ACL command line |
| Windows host exception text → transport_failed detail | Exception messages from Add-Type, the compiler and Win32 cross into the error the CLI prints, the JSON envelope, CI logs and run files |

---

## Threat Register

| Threat ID | Plans | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|-------|----------|-----------|----------|-------------|------------|--------|
| T-25-01 | 25-01 | Tampering | capture25.sh writes | high | mitigate | `assert_local` before any request (loopback only, the WR-08 authority cut); `c8del` refuses any name that is not `CAP25-*`; acceptance tests the refusal | closed |
| T-25-02 | 25-01 | Information disclosure | WHK-Global content in captures | high | mitigate | `members` prints names, sizes and sha256 only; acceptance greps for token headers; the source checkout is copied, never modified (git status compared before and after) | closed |
| T-25-03 | 25-01 | Information disclosure | API tokens | medium | mitigate | tokens sourced from `$STATE/tokens.env` (0600, outside the repo), `cap` prints the variable name only, no `set -x` | closed |
| T-25-04 | 25-02 | Tampering | C-H test setup on capture hosts | high | mitigate | Only `CAP25-GITPROBE` and its `.git` entries are created, by the recorded admin route, and removed in the same task; probes run as the deploy account with pinned known_hosts | closed |
| T-25-05 | 25-02 | Elevation of privilege | grant text | medium | mitigate | The proposed grant is list/traverse only (`r-x`), no write right, written for the user to apply in 25-03; this plan applies no ACL | closed |
| T-25-06 | 25-02 | Tampering | C10 module writes | medium | mitigate | C10 runs on a disposable loopback gateway with a bare repo under `$STATE`; WHK-Global's repo is read, never written (git status compared) | closed |
| T-25-07 | 25-02 | Information disclosure | captures file | medium | mitigate | Names, statuses, timings only; acceptance greps for private keys and token headers | closed |
| T-25-08 | 25-03 | Repudiation | contract decisions | medium | mitigate | Blocking checkpoint; the answer is recorded verbatim with a UTC date under `## Contract decisions (25-03)`; later plans assert it as a precondition | closed |
| T-25-09 | 25-03 | Information disclosure | pasted git.yaml | high | mitigate | Only `production_*`, `commissioning_*`, `enforceBranch` keys are requested; acceptance greps the captures for password, secret, token and userinfo URLs | closed |
| T-25-10 | 25-03 | Elevation of privilege | host grant | medium | mitigate | Grant text is list/traverse only (`r-x`), no write right; the user applies or explicitly authorizes it; the applied lines are recorded | closed |
| T-25-11 | 25-04 | Information disclosure | `build_artifact` walk | high | mitigate | `symlink_metadata`, never follow; symlinks and special files refuse `deploy_tree_invalid`; only allowlisted top-level directories and `resource.json`-listed files enter the zip (K2, K3) | closed |
| T-25-12 | 25-04 | Tampering | project name from `deploy/project.yaml` | medium | mitigate | `component_problem` and `windows_component_problem`; the REST path is percent-encoded by the existing client | closed |
| T-25-13 | 25-04 | Elevation of privilege | MCP exposure | high | mitigate | `EXCLUDED_PATHS` full-path entry plus `deploy_projects_is_excluded_by_full_path` | closed |
| T-25-14 | 25-04 | Tampering | `--check` writes | medium | mitigate | `ProjectGateway` exposes reads only in this slice; wiremock tests assert GET only; no run directory code on the check path | closed |
| T-25-15 | 25-04 | Information disclosure | output | low | accept | Output carries names, hashes and counts only; the full canary suite lands in 25-17 | closed |
| T-25-16 | 25-06 | Information disclosure | previous run's pre-image export in `.ign/` | high | mitigate | `BUILTIN_DENY` excludes `.ign` at any depth (test `builtin_deny_any_depth`) on top of the K3 allowlist | closed |
| T-25-17 | 25-06 | Denial of service | huge or deep checkout | medium | mitigate | `ARTIFACT_MAX_ENTRIES` and `IMPORT_MAX_BYTES` refusals before any gateway contact | closed |
| T-25-18 | 25-06 | Tampering | silent partial import (Pitfall 1) | high | mitigate | listed-but-missing and empty-artifact refusals; `validate_import` on the built bytes | closed |
| T-25-19 | 25-06 | Tampering | `.ignexclude` patterns escaping the repo | low | mitigate | absolute paths, `..` and control characters refuse with the line number; matching is on repo-relative strings only | closed |
| T-25-20 | 25-07 | Tampering | `.git` probe silently off | high | mitigate | Fail closed: a missing `projects` directory or an unreadable project directory is `transport_failed`; tests `local_git_entry_kinds`, `git_probe_unreadable_is_transport_failed` | closed |
| T-25-21 | 25-07 | Tampering | project name as a path segment | medium | mitigate | `component_problem` before any path is built (canon path traversal also tracked by /gsd-secure-phase) | closed |
| T-25-22 | 25-07 | Elevation of privilege | host write from a read verb | medium | mitigate | `ProjectHost` has no write or delete method (acceptance grep); the write trait arrives separately in 25-11 with a confinement scan | closed |
| T-25-23 | 25-07 | Information disclosure | refusal text | low | accept | The message carries project name, location and kind only | closed |
| T-25-24 | 25-08 | Tampering | Windows probe misses a Hidden `.git` | high | mitigate | `GIT_ENTRY` reads attributes with the C-H-proven form; unit test for hidden and reparse cases; live Windows row | closed |
| T-25-25 | 25-08 | Tampering | PowerShell injection through the project name | medium | mitigate | fixed script text, path passed as a parameter, `windows_component_problem` before use (canon injection also tracked by /gsd-secure-phase) | closed |
| T-25-26 | 25-08 | Spoofing | host key from an unreviewed file | medium | mitigate | `known_hosts` resolves under `--targets` and the existing `known_hosts_problem` refuses escapes | closed |
| T-25-27 | 25-08 | Elevation of privilege | broader grant than needed | low | mitigate | README documents list-only rights; no write right under `data/projects` | closed |
| T-25-28 | 25-09 | Tampering | corrupt or foreign record disabling the gates | high | mitigate | `ReleaseRecord::parse` strict (schema, gateway, project, hash, run id, size); any anomaly `deploy_run_invalid`, never absent; tests `release_record_parse` | closed |
| T-25-29 | 25-09 | Repudiation | override flags | medium | mitigate | `allow_downgrade` and the `allow_first_change` reason are reported in the result (and recorded in `run.json` from 25-11) | closed |
| T-25-30 | 25-09 | Tampering | mode collection colliding with record path | low | mitigate | `Target::validate` reserves mode `projects` | closed |
| T-25-31 | 25-10 | Denial of service | parent chain walk | medium | mitigate | bound at `projects().len() + 1` finds, `cycle` when exceeded (test `parent_chain_is_bounded`) | closed |
| T-25-32 | 25-10 | Tampering | child deploy touching the parent | high | mitigate | `ProjectGateway` has no write besides the project's own import (25-11); test `child_check_reads_parent_only` pins the call log | closed |
| T-25-33 | 25-10 | Tampering | hostile parent name | low | mitigate | the name travels as one percent-encoded REST segment through the existing client; never a path or shell argument | closed |
| T-25-34 | 25-11 | Spoofing | unreviewed artifact uploaded | high | mitigate | Decision C before any contact; `--artifact-sha256` compared with the local build before host or gateway contact; test `projects_apply_hash_mismatch_uploads_nothing` | closed |
| T-25-35 | 25-11 | Tampering | data loss on overwrite (Pitfall 1) | high | mitigate | pre-image export before import; `plan_stale` when the live project moved; post-import verify; no automatic second import; restore command printed (25-12, T-25-64) | closed |
| T-25-36 | 25-11 | Information disclosure | pre-image export on disk | high | mitigate | `create_private_file` (0600) inside the 0700 run directory under a git-ignored `.ign/`; never echoed; canary suite in 25-17 | closed |
| T-25-37 | 25-11 | Elevation of privilege | record writer reachable from read verbs | medium | mitigate | `write_half_unreachable` plus `release_writer_is_confined`; the CLI never names the writer | closed |
| T-25-38 | 25-11 | Repudiation | who deployed what | low | mitigate | `run.json`, `plan.json`, `events.ndjson`, `result.json` per run; the record names the run id | closed |
| T-25-39 | 25-11 | Tampering | write to the backup node | medium | mitigate | the action only uses `nodes.master`; no backup session is resolved in the `Projects` arm | closed |
| T-25-40 | 25-13, 25-20, 25-21 | Tampering | torn or half-written record | high | mitigate | temp file plus atomic replace; failure removes the temp; readers fail closed on anything unparseable (25-09) | closed |
| T-25-41 | 25-13 | Tampering | symlink redirecting the write | high | mitigate | every component listed and checked before the `put`; `release.json` must be absent or regular | closed |
| T-25-42 | 25-13, 25-20, 25-21 | Tampering | command injection in the move | medium | mitigate | fixed argv `mv -f --` through `RemoteCommand::posix` with validated components; Windows fixed script with parameters (canon injection also tracked by /gsd-secure-phase) | closed |
| T-25-43 | 25-13 | Elevation of privilege | writes outside the account home | medium | mitigate | the writer's only target is `release_record_rel` below `account_home()`; `release_writer_is_confined` keeps it out of read-only surfaces | closed |
| T-25-44 | 25-14 | Spoofing | write to the wrong gateway (profile points elsewhere) | high | mitigate | `wrong_target` compares REST name, host `systemName` and the declared `gateway_name` before any write, `record` included | closed |
| T-25-45 | 25-14 | Tampering | import under an open Designer session | medium | mitigate | `designers_gate` with `fail` default; `proceed` recorded in `run.json` and events | closed |
| T-25-46 | 25-14, 25-20, 25-21 | Repudiation | proceeding over sessions | high | mitigate | sessions listed by name in `run.json` | closed |
| T-25-47 | 25-15 | Tampering | writes to children | high | mitigate | the sweep calls only reads (test `sweep_only_reads`); `ProjectGateway` exposes no write besides the deployed project's import | closed |
| T-25-48 | 25-15 | Denial of service | endless settle | low | mitigate | bounded by `children_settle`; classification on the last read with a warning | closed |
| T-25-49 | 25-15 | Information disclosure | child content in output | low | mitigate | reports carry names, statuses, releases and hashes only | closed |
| T-25-50 | 25-16 | Spoofing | host key file from an unexpected place | medium | mitigate | `known_hosts` resolves only under DIR and `known_hosts_problem` refuses escapes; the CI checks the IaC repo out read-only with a same-org token (25-18) | closed |
| T-25-51 | 25-16 | Tampering | regression in existing deploy verbs | medium | mitigate | additive entry points with `targets = repo` defaults; every existing deploy suite runs unchanged in verify | closed |
| T-25-52 | 25-17 | Information disclosure | outputs and run files | high | mitigate | canary suites over every output channel, run file and record, success and refusal paths; one named exempt file asserted 0600 | closed |
| T-25-53 | 25-17 | Tampering | two deployers for one tag after cutover | medium | mitigate | runbook step disabling the git module's tag pull per project (spec section 13 step 8) | closed |
| T-25-54 | 25-17 | Tampering | cutover changing a project silently | medium | mitigate | runbook requires `--check` action `record` (D-12) before the first apply | closed |
| T-25-55 | 25-18 | Elevation of privilege | untrusted tag or branch deploying to prod | high | mitigate | the apply job runs under `environment:` so GitHub environment protection rules (required reviewers, tag rules) gate prod; documented in the README caller | closed |
| T-25-56 | 25-18 | Information disclosure | IaC token | high | mitigate | dedicated secret, read-only scope, `persist-credentials: false`, never echoed (harness grep) | closed |
| T-25-57 | 25-18 | Tampering | apply not bound to the reviewed check | high | mitigate | `--artifact-sha256` from `needs.check.outputs`; Decision C makes a missing hash `invalid_input` | closed |
| T-25-58 | 25-18 | Tampering | concurrent runs of one project | medium | mitigate | concurrency group per environment and repository, `cancel-in-progress: false` | closed |
| T-25-59 | 25-18 | Information disclosure | step summary | low | mitigate | the summary is built from the JSON envelope, which carries names, hashes and counts only (canary suite, 25-17) | closed |
| T-25-60 | 25-19 | Tampering | live tests against a real gateway | high | mitigate | loopback-only URL guard, `IGNITION_LIVE_DEPLOY=1` opt-in, `CAP25-*` names only, cleanup in every test | closed |
| T-25-61 | 25-19 | Information disclosure | live records | low | mitigate | captures hold outcomes, hashes and counts only; token from the env, never printed | closed |
| T-25-62 | 25-05 | Tampering | `FakeProjects` diverging from the gateway | medium | mitigate | the module doc cites the C8 block for every behaviour and names where the captures are silent; the export stamps `lastModification*` as C8b recorded so normalisation is exercised; the 25-19 live gates repeat the decisions on the rig | closed |
| T-25-63 | 25-05 | Information disclosure | README examples | low | mitigate | examples use the placeholder project `CapApp` and a `dev` environment; no host names, tokens or project file content | closed |
| T-25-64 | 25-12 | Tampering | unverified import treated as deployed | high | mitigate | `data.ok: false`, `data.slug`, exit 6 after the envelope and the same in `result.json`; test `projects_apply_unverified_exit_6`; the 25-18 report step fails the job on exit 6 | closed |
| T-25-65 | 25-12 | Information disclosure | progress lines, human output, restore command | medium | mitigate | names, hashes and the run-directory path only; `StderrProjectsObserver` never formats file content; canary suite in 25-17 | closed |
| T-25-66 | 25-12 | Tampering | automatic restore (a second overwrite import) | high | mitigate | none is sent: `projects_apply_unverified_exit_6` asserts a single POST; the operator runs the printed restore command (K11) | closed |
| T-25-67 | 25-21 | Information disclosure | RECORD_REPLACE cause text in the transport_failed detail | medium | mitigate | IgnLine replaces every drive-letter or UNC path with `<path>`, maps non-ASCII to `?` and controls to a space, and caps the cause at 200 characters. The live Add-Type test asserts no account-home native path, no `AppData` and no colon-backslash in the detail; the blocked test asserts no home path | closed |
| T-25-68 | 25-21 | Denial of service | a helper name shadowed by a built-in alias, or a helper throwing inside the catch, hiding the cause | low | mitigate | The helper names IgnLine and IgnCause match no built-in alias (aliases outrank functions; `r` is Invoke-History). IgnCause has its own try and catch, and the Win32 message lookup has its own try | closed |
| T-25-69 | 25-21 | Spoofing | capture-host key re-pin after the IP change | medium | mitigate | The session known_hosts is written only when the scanned ed25519 blob equals the already-pinned blob; a mismatch stops the run. StrictHostKeyChecking=yes stays on, and the session file is removed after the run | closed |
| T-25-70 | 25-21 | Information disclosure | live test output | low | mitigate | CAPTURE lines carry details, hashes and timings only. The key is passed as text in the env and never printed (existing file convention) | closed |
| T-25-71 | 25-22 | Tampering | release.yml Windows build: a static-CRT flag silently dropped (RUSTFLAGS overriding a config file, or a later edit) | medium | mitigate | The flag goes through the action's `rustflags` input, which becomes RUSTFLAGS. The import check fails the job before packaging when any vcruntime or msvcp import remains, so a dynamically linked binary is never uploaded. release-test.sh pins the flag placement and the step order | closed |
| T-25-72 | 25-22 | Repudiation | a vacuous import check (dumpbin output format changes, empty list passes) | low | mitigate | The check requires KERNEL32.dll in the parsed list before it judges anything, and prints the full list to the log and the job summary | closed |
| T-25-73 | 25-22 | Tampering | build tools taken from the runner image (vswhere, dumpbin) | low | accept | First-party Microsoft tools on the GitHub-hosted image the release already trusts for the compiler and linker; nothing new is downloaded | closed |
| T-25-74 | 25-23 | Information disclosure | key file and run directory left readable when the ACL step fails | high | mitigate | Every failure (whoami spawn or exit, unparsable SID, icacls failure) refuses: transport_failed before ssh is spawned, or an io error before the run directory is used. There is no fallback to a name or to an unrestricted file | closed |
| T-25-75 | 25-23 | Tampering | argument injection through the SID text into icacls | medium | mitigate | is_sid accepts only `S-1-` and dash-separated digit groups, and icacls_owner_only_args refuses anything else. The grant is one argument built from a validated SID and a fixed rights string | closed |
| T-25-76 | 25-23 | Spoofing | a planted whoami.exe or icacls.exe | low | accept | Rust's Command resolves the system directory before PATH on Windows. A planted copy beside ign.exe needs write access to the install directory, which already means the runner is compromised | closed |
| T-25-77 | 25-23 | Information disclosure | a service-account SID is shared by every service running as that account | medium | transfer | A key restricted to NETWORK SERVICE or LocalSystem is readable by every service on the host running as that account. Plan 25-26's README tells operators to run the runner service as a dedicated user, and states this trade-off | closed |
| T-25-78 | 25-24 | Elevation of privilege | writePermissions holding bare Authenticated, so every API key (including the check token) can write | high | mitigate | adopt never adds bare Authenticated, bare Public or an empty list to writePermissions (Task 2, with unit tests). `--read-only` refuses to mint on an open list and proves 403 on the rescan. `--restrict-write` removes the entry. The live end-to-end test proves the read-only key gets 403 after the removal | closed |
| T-25-79 | 25-24 | Elevation of privilege | an empty writePermissions after a removal (Ignition reads `[]` as Public) | high | mitigate | remove_bare_authenticated_from_write refuses, before any PUT, when the result would be empty (unit-tested); write_admits_every_key flags `[]` | closed |
| T-25-80 | 25-24 | Denial of service | `--restrict-write` cutting off other integrations' keys, or locking out the administrator | medium | mitigate | Opt-in only. The warn row names the entry and the consequence. The minted key's write and the admin session PUT are re-proven after the removal in the live test. README gives the undo path | closed |
| T-25-81 | 25-24 | Tampering | lost update on the security singleton (whole-config PUT) | medium | mitigate | Unchanged discipline: GET then merge then PUT with the GET's signature (optimistic concurrency). The capture's set_state re-reads and checks the lists it wrote | closed |
| T-25-82 | 25-24 | Information disclosure | minted keys in test output or capture files | medium | mitigate | Keys live in memory only. CAPTURE lines carry level names, status codes and booleans. Teardown deletes the test profiles' keyring entries | closed |
| T-25-83 | 25-24 | Spoofing | a check token assumed read-only without proof | medium | mitigate | adopt's read-only probe (403 required) and the README's `ign doctor --check-write` verification | closed |
| T-25-84 | 25-25 | Information disclosure | write-capable token or key reaching the unapproved check job | high | mitigate | The check action has no write-capable input and no secrets reference (3d2). The workflow check job references only the check secrets and the targets token (3d). Both are checked by exact-set harness cases | closed |
| T-25-85 | 25-25 | Tampering | the apply deploying an artifact or target other than the checked one | high | mitigate | apply-handoff refuses a missing or malformed hash or commit before any checkout (4f). The targets checkout is at targets-sha and verified (4d). ign refuses a different artifact (project_hash_mismatch; e2e 5b) | closed |
| T-25-86 | 25-25 | Tampering | deploy-projects.yml running actions from a different commit than itself | high | mitigate | Checkout at job.workflow_sha, HEAD verified equal (4h). An empty or malformed value fails before the checkout, so actions/checkout never falls back to the default branch | closed |
| T-25-87 | 25-25 | Tampering | CRLF rewriting of the project or targets on Windows, changing the artifact hash or breaking the tree | medium | mitigate | GIT_CONFIG_COUNT/KEY_0/VALUE_0 on every checkout and every git or ign step (3f), proven against an autocrlf=true config (4e) | closed |
| T-25-88 | 25-25 | Tampering | script injection through inputs | high | mitigate | Every input reaches bash or JS through `env:`; body_checks forbids `${{` in body regions of all three files (1b); shellcheck on action bodies (1c) | closed |
| T-25-89 | 25-25 | Information disclosure | secrets echoed to logs | medium | mitigate | The echo/xtrace scan covers all three files (3c). The handoff and secrets bodies refuse without printing values (4f, 4g grep the log for the token) | closed |
| T-25-90 | 25-25 | Repudiation | a cross-org caller of deploy-projects.yml failing silently with empty secrets | medium | mitigate | apply-secrets fails first, naming the inherit and same-org cause and the composite actions (4g) | closed |
| T-25-91 | 25-26 | Information disclosure | a template that references write secrets in a check job, or stores them as repository secrets | high | mitigate | readme-test.sh 6b asserts no write-secret reference and no `environment:` in check jobs, and write secrets only in apply jobs. The secrets table tells operators to store the write secrets as environment secrets and why | closed |
| T-25-92 | 25-26 | Spoofing | an unintended runner taking the deploy job (non-unique labels) | medium | mitigate | Every example carries a unique label (6c and the Task 2 grep), with the reason stated | closed |
| T-25-93 | 25-26 | Elevation of privilege | a check token that can write | high | mitigate | The secrets table requires the read-only token from the adopt procedure and its `ign doctor --check-write` 403 verification (plan 25-24 enforces it in adopt) | closed |
| T-25-94 | 25-27 | Information disclosure | private gateway URLs, hosts, labels or targets refs in committed UAT files | medium | mitigate | The callers read repository variables of the private UAT repo. Task 1's acceptance greps both files for URLs (0). Captures record run ids, hashes and outcomes only. The untracked `svc-ignition-deploy@*.ps1` is never staged | closed |
| T-25-95 | 25-27 | Information disclosure | a token or key in a run log | high | mitigate | Step 8 greps every log for private-key headers and the check-token pattern (both 0); masking is checked on the env lines | closed |
| T-25-96 | 25-27 | Information disclosure | a token printed while it is copied into GitHub | medium | mitigate | `security find-generic-password -w` piped straight into `gh secret set`; nothing is echoed or written to a file | closed |
| T-25-97 | 25-27 | Spoofing | users installing the temporary UAT build (route a) | low | mitigate | `--prerelease --latest=false`, a title and notes saying it is not a release, deleted with its tag at the end (step 11) | closed |
| T-25-98 | 25-27 | Elevation of privilege | the VM left with the runner as NETWORK SERVICE and the VC redist removed | low | accept | The VM is a disposable test host. It is deallocated after the run, and its state is recorded so the next session knows it | closed |
| T-25-99 | 25-27 | Spoofing | a different runner taking the UAT jobs | medium | mitigate | The unique label variable; the diagnostics step proves the host by the account SID and the VM's own paths | closed |
| T-25-SC | 25-25 | Tampering | third-party actions used inside the composite actions (actions/checkout@v7, actions/github-script@v9, actions/upload-artifact@v7) | low | accept | The same GitHub-owned actions, at the same major tags, that deploy-projects.yml already used; no new package or action is introduced | closed |
| T-25-F1 | 25-18 (flag) | Elevation of privilege | `deploy-plan.yml` ungated `plan` job maps `IGN_SSH_KEY`, the write-capable key the environment-gated apply in `deploy-apply.yml` also uses | high | accept | Accepted risk AR-01: predates phase 25 (quick task 261021), tracked as a carry-forward in STATE.md | closed |
| T-25-F2 | 25-17 (flag) | Information disclosure | `ImportDenied` copies the gateway's `problem` text verbatim and unbounded into the message, the hint, the JSON envelope and the CI step summary (`client/projects.rs:263-275`, `error.rs:134`, `:1444-1448`) | low | mitigate | Cap the length and strip non-ASCII. Not implemented. Controls are escaped in human output (`render.rs` `escape_error_text`), and run files record the slug only | open — below high threshold (non-blocking) |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

### Verification notes

- **Evidence:** the 2026-10-29 audit cited a file:line or a test function for every closed threat, and confirmed each cited test name exists with grep. Accepted threats (T-25-15, T-25-23, T-25-73, T-25-76, T-25-SC, T-25-98) were checked to confirm the accepted condition still holds. The transfer T-25-77 is documented in the README's dedicated-user section.
- **T-25-69 and T-25-96** are closed on procedural evidence: the 25-LIVE-CAPTURES rows for the pinned host key and for token minting without echo.
- **T-25-84** is closed as declared, but the README's project-deploy secrets section offers storing the same write-capable deploy key as `IGN_CHECK_SSH_KEY`, and the read-restricted alternative is marked "not exercised". An operator who takes that option recreates T-25-F1's exposure for project deploys.
- **Untracked file:** `svc-ignition-deploy@52.154.75ace-uat4.ps1` at the repo root is untracked and not git-ignored. T-25-94 holds because it was never staged, but a `git add -A` would commit it.

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-25-F1 | Predates phase 25: the reusable `deploy-plan.yml` came from quick task 261021, and plan 25-18 only added targets inputs. A reusable-workflow caller cannot put `environment:` on the calling job, and README says the plan's `IGN_SSH_KEY` must be a repository or organization secret, so anyone who can run a workflow in the caller repo can reach a key that writes the gateway's config collection before approval. Follow-up: add `IGN_PLAN_SSH_KEY` for a read-restricted key, keep `IGN_SSH_KEY` environment-only, and add a harness check like 3d in `deploy-projects.yml` | user (operator decision in /gsd-secure-phase 25) | 2026-10-29 |
| AR-02 | T-25-15, T-25-23, T-25-73, T-25-76, T-25-SC, T-25-98 | Accepted at plan time; rationale in each plan's threat model. Conditions re-checked 2026-10-29 and still hold | planner, confirmed by audit | 2026-10-29 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-29 | 102 (100 registered + 2 flags) | 101 | 1 (T-25-F2, low, non-blocking) | gsd-security-auditor (opus), ASVS L1, block_on high |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-29
