---
status: diagnosed
phase: 07-ecosystem-interop-advanced-ops
source: 07-01-SUMMARY.md, 07-02-SUMMARY.md, 07-03-SUMMARY.md, 07-04-SUMMARY.md
started: 2026-08-29T13:03:04Z
updated: 2026-08-29T15:05:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Cross-gateway project diff
expected: With two profiles (A, B) pointing at different gateways, `ign project diff <A> <B> --project <NAME>` lists per-member statuses grouped as same/added/removed/changed, B-relative-to-A (added = in B only, removed = in A only). Identical resources across gateways report `same` (lastModification churn normalized). JSON carries profile_a/profile_b in data.
result: pass
note: >-
  Automated by Claude on two live gateways (rig A = ignition-devops :9088, clone B = uatb :9188,
  token provisioned per the 04-VERIFICATION recipe, volume clone inherits it). Fixture:
  shared.py put independently on both sides -> `same` (normalization proven); only_a.py ->
  `removed`; only_b.py -> `added`; diverged.py -> `changed`; scripts/resource.json honestly
  `changed` (folder descriptors differ — real content difference). JSON: scope=project,
  profile_a/profile_b in data, envelope profile = active (uat), summary counts correct.
  Same-profile refusal `diff uat uat` -> exit 2 invalid_input (no-op message). Minor
  observation: the invalid_input refusal hint is the frozen shared --file/stdin text
  (documented 07-01 behavior) — cosmetic only, message itself is clear.

### 2. Project sync guard + A→B promotion
expected: `ign project sync <A> <B> --project <NAME> --all-changed` WITHOUT --yes refuses: exit 2, profile null, zero network requests, hint names the whole-project overwrite-import consequence. WITH --yes, A's changed/missing resources land in B (re-diff shows them same). `--all-changed` when nothing changed performs NO import (exit 0, empty synced/removed lists). `--delete` opt-in removes B-only members.
result: pass

### 3. Standalone backup download
expected: `ign backup download` fetches a .gwbk from any profiled gateway (default export-convention filename like `<host>-<ts>.gwbk`; `-o FILE` overrides; `--type roaming|all` selects scope). Human + JSON modes both work.
result: pass

### 4. Backup restore is guarded
expected: `ign backup restore <FILE>` WITHOUT --yes refuses (exit 2, profile null, operation string names restart-block consequence). WITH --yes restores the gwbk — thin: no bundled wait, gateway restarts on its own (README documents the window). Missing/empty file exits 2 pre-network.
result: pass
reported: "user ran live: `ign backup restore Ignition-Git_Ignition-backup-20260829-1450.gwbk --yes` -> 'Restored — the gateway restarts now (blocked for ~minutes)' — thin restore + restart-block message confirmed on gateway A"
note: >-
  User executed the --yes path live on the uat profile; human render names the restart block
  verbatim. Gateway A now restarting from the restore (~minutes).

### 5. EAM reads + controller gate
expected: `ign eam history` lists EAM task history (outcomes like Failed/GNET-not-connected ride as data, never hidden); `ign eam tasks [NAME]` lists config-resource definitions. On a non-controller gateway the 403 surfaces as `eam_not_controller` (exit 6) — NOT mislabeled `auth_rejected`.
result: issue
reported: "ign eam history -> 'error: internal error: response from http://localhost:9088/data/eam/api/v1/eam-tasks/history did not match the expected shape: error decoding response body' (hint: internal errors are bugs). ign eam tasks -> WORKS: 'cli-research-backup type=eam_backup schedule=OnDemand state=-'"
severity: blocker
note: >-
  User ran live on gateway A (controller-mode — has EAM task cli-research-backup from earlier
  e2e work; the non-controller 403 gate was therefore not exercised here). eam tasks passes;
  eam history fails to decode the real controller response. Raw wire response captured to
  .planning/debug/eam-history-raw.json: {items:[{taskId,taskName,taskStart(ms int),taskEnd,
  target,level,detail,taskType}],metadata:{total,matching,limit,offset}} — decode mismatch
  against the client model's expected shape.

### 6. EAM task new guard ladder
expected: `ign eam task new --type backup` (OnDemand default) runs unguarded. Mutating types (e.g. send-history, license) demand --yes without it → exit 2 refusal. restore/install/upgrade types are REFUSED outright (`eam_task_type_refused` exit 6). K=V settings auto-type scalars (bool/int); --definition deep-merges over the profile.
result: issue
reported: "ign eam task new uat-backup-demo eam_backup -> 'error: internal error: unexpected HTTP 422 Unprocessable Entity from http://localhost:9088/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks'. fail. also include needing better help"
severity: blocker
note: >-
  Claude replicated via curl: gateway answers 422 {"messages":["Settings cannot be null"]} —
  the composed body puts targetGateways in config.profile and omits config.settings entirely,
  but the live contract requires config.settings (see .planning/debug/eam-create-422.md and
  eam-working-definition.json). Secondary taxonomy gap: the 422 falls through classify() to
  internal_error. Guard-rung refusals not separately verified (create rung failed first);
  help-UX feedback logged as its own gap below.

### 7. EAM task force sequence
expected: `ign eam task force <NAME>` runs find (owner resolution) → force POST → history re-read; the outcome of the forced task rides as data in the result.
result: issue
reported: "error: internal error: unexpected HTTP 409 Conflict from http://localhost:9088/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup (gateway error page: Error 409: Task 'cli-research-backup (forced)' already exists! It must be completed or deleted before another task of this type can be force executed.)"
severity: blocker
note: >-
  Two-layer defect: (a) the gateway state legitimately blocks re-force (a leftover '(forced)'
  run from the Phase 7 research occupies the slot) — but the CLI surfaces the 409 as
  internal_error instead of a target-state refusal (the session_not_prunable 409 precedent,
  force-route edition); (b) the error page text DOES ride in the message (informative) but
  the classification is wrong. Owner resolution + find worked (owner=eam in the URL).

### 8. script run happy path
expected: After `ign webdev deploy --with-script-exec`, `ign script run --code "print('hello')"` returns an envelope with {stdout, result, elapsedMs} — ALL keys always (empty/null/0 defaults when absent). Human mode prints the stdout block verbatim plus result/elapsed lines.
result: pass
reported: "user ran live: stdout: hello / result: null / elapsed: 7 ms — all keys present, stdout verbatim"
note: >-
  Side observations from the same session: `ign rig trial reset --yes` worked on A (expired
  true -> false, 1h59m remaining); the eam task new 422 reproduced consistently (gap 3).

### 9. script run structural gate + input forms
expected: Without scriptExec deployed, `ign script run --code "..."` exits 6 (`script_exec_not_configured`) with the hint naming `ign webdev deploy --with-script-exec` verbatim, zero HTTP. Both --code AND --file given → exit 2 invalid_input (envelope, not clap usage). `--file -` reads the script from stdin.
result: pass
reported: "user ran live on uat-b (no stored secret): 'error: scriptExec is not configured for profile "uat-b" — the secret-gated route deploys only via the explicit opt-in' + hint naming ign webdev deploy --with-script-exec verbatim"
note: >-
  Structural gate verified live (zero HTTP, profile-tagged message, flag-in-hint). The
  stdin / both-inputs sub-checks were not shown in the session paste but are binary-golden
  pinned in contract_script.rs (07-03).

### 10. Decode/encode scripts round-trip
expected: `ign project export <NAME> --decode-scripts` writes the member tree PLUS counter-named `<member>.<n>.py` sidecars + scripts-manifest.json. Editing a sidecar then `ign project import --file <dir> --encode-scripts` splices edits back. UNEDITED members round-trip byte-identical.
result: pass
note: >-
  Two loops verified live on gateway A. (1) User's manual loop: export --decode-scripts ->
  edit only_a.py -> import --encode-scripts -> resource get shows 'only on A - New' landed.
  (2) Claude's crafted-Flint loop (diffdemo's .py members are plain text, so Claude put a
  real encoded member transforms/demo.json with \u0027/\t/\n escapes under a 'transform'
  key): export --decode-scripts -> '1 scripts decoded', sidecar demo.json.1.py with dedented
  plain Python + scripts-manifest.json (pointer /params/transform, indent_prefix \t) ->
  edit return 7->99 -> import --encode-scripts -> raw member on the gateway carries the edit
  with \u0027 escapes and tab indent INTACT (span-level splice fidelity confirmed by
  re-export; resource get's plain-quote display is just re-serialization). Unedited
  shared.py/diverged.py byte-identical across the round-trip (cmp).

### 11. ign lint delegation
expected: With ignition-lint on PATH: `ign lint <path>` exits 0 when the tool ran, findings + report ride as data; `--strict` passes the child's exit code through. WITHOUT the tool on PATH: exit 6 `lint_tool_absent` with a uv/pip install hint.
result: pass
reported: "user ran live (ignition-lint on PATH): 'lint: 0 issue(s), child exit 0 / summary: {} / Found 3 Python script files / Processing file 3/3...' — doctor posture, child stdout rides through, exit 0"
note: >-
  With-tool path verified live (3 .py files found = the diffdemo scripts tree). --strict
  passthrough and the absent-tool exit-6 hint are binary-golden pinned (contract_lint.rs).

### 12. tags browse --from-export (offline)
expected: `ign tags browse --from-export <path>` browses a git-module export dir, legacy `<provider>.json` file, or CLI interchange file fully OFFLINE — no gateway needed (profile null in JSON). Tree + flat renders reuse the existing browse surfaces.
result: pass
note: >-
  Claude-run offline (user's own attempt proved the missing-path refusal: invalid_input exit 2).
  All three layouts verified: git-module dir (provider root, _types_ -> UdtType, %2F fs-name
  decode pump%2F1.json -> pump/1, dot-entry skips), legacy <provider>.json whole tree
  (provider = file stem), CLI interchange file (real subtree export from gateway B, renamed
  default.json -> provider 'default'). JSON envelope: profile null, source 'export'. Side
  finding during prep (gap 5): provider-ROOT exportTags/getConfiguration crash the deployed
  tagConfig route with 'No RpcContext available' on both gateways; subtree paths work.

## Summary

total: 12
passed: 9
issues: 3
pending: 0
skipped: 0

## Gaps

- truth: "eam history lists task history with outcomes riding as data"
  status: failed
  reason: "User reported: error: internal error: response from http://localhost:9088/data/eam/api/v1/eam-tasks/history did not match the expected shape: error decoding response body (eam tasks works fine)"
  severity: blocker
  test: 5
  root_cause: "EamHistoryItem.task_id is declared i64 but the real 8.3.3 controller serializes taskId as a UUID STRING (raw capture: "taskId":"a2f4dab1-..."); serde default does not rescue type mismatches, so reqwest .json() decode fails -> InternalError. Also the history list envelope is {items,metadata} — items decode is where it breaks."
  artifacts: ['crates/ignition-core/src/client/eam.rs (EamHistoryItem.task_id type)', 'crates/ignition-core/src/client/eam.rs unit test fixture (taskId: 42 number — wrong image)']
  missing: [Change task_id to String (wire-faithful), fix the unit fixture, add a contract fixture shaped like .planning/debug/eam-history-raw.json]
  debug_session: ".planning/debug/eam-history-raw.json"

- truth: "eam task new help lets a first-time user form a correct command without trial and error"
  status: failed
  reason: "User reported: 'Lets make the help much more helpful' — fumbled three invocations (missing TYPE positional, --eam_backup as flag, -- --eam_backup) before discovering NAME-then-TYPE order; the TYPE doc string says 'the openapi taxonomy: eam_backup, eam_restart, …' without enumerating valid values, and the bare <NAME> <TYPE> usage line doesn't communicate which word is which"
  severity: minor
  test: 6
  root_cause: "TYPE is a bare String positional; the taxonomy lives only in a prose doc comment. No clap possible-values, no example invocation in the doc."
  artifacts: ['crates/ignition-cli/src/cli.rs (EamTaskNewArgs type doc)']
  missing: [Enumerate the type classes in the TYPE doc (benign/mutating/refused with full token lists) + add a worked example line to the doc; consider clap value hints]
  debug_session: ""

- truth: "eam task new creates a valid definition on a real controller gateway"
  status: failed
  reason: "User reported: error: internal error: unexpected HTTP 422 Unprocessable Entity — the composed create body is rejected by the real gateway ('Settings cannot be null')"
  severity: blocker
  test: 6
  root_cause: "eam_task_new composes config.profile={type,scheduleMode,targetGateways,...} with NO config.settings, but the real 8.3.3 controller requires config.settings non-null ('Settings cannot be null') and expects targetGateways INSIDE settings (see the working cli-research-backup definition: config.settings={targetGateways,targetGroups,concurrentBackups,forceBackups}). Secondary: the gateway's 422 falls through classify() to InternalError."
  artifacts: ['crates/ignition-core/src/actions/eam.rs (eam_task_new composition)', '.planning/debug/eam-create-422.md', '.planning/debug/eam-working-definition.json']
  missing: [Compose config.settings (targetGateways/targetGroups defaults per the live definition shape), keep --setting K=V landing in settings, add a classify arm for 422 on the config-resource create path (or map to invalid_input), re-pin wiremock contracts to the live shape]
  debug_session: ".planning/debug/eam-create-422.md"

- truth: "eam task force surfaces state conflicts as target-state refusals, never internal errors"
  status: failed
  reason: "User reported: error: internal error: unexpected HTTP 409 Conflict ... Task 'cli-research-backup (forced)' already exists! It must be completed or deleted before another task of this type can be force executed."
  severity: blocker
  test: 7
  root_cause: "A genuine gateway state conflict (leftover '(forced)' run occupies the slot), but the force route's 409 has no classify arm — it falls through to InternalError instead of a target-state refusal. The session_not_prunable precedent (06-07) already established path-scoped 409 arms."
  artifacts: ['crates/ignition-core/src/client/classify.rs (no 409 arm on /data/eam/api/v1/eam-tasks/force/*)', 'crates/ignition-core/src/error.rs (additive slug needed)']
  missing: [Add a path-scoped 409 classify arm -> additive exit-6 slug (e.g. eam_task_in_flight) carrying the gateway's page text, two-place exit-table rule + README sync]
  debug_session: ""

- truth: "tags export/config get on a provider root works or refuses honestly (subtree paths already work)"
  status: failed
  reason: "Claude live-probe during test 12 prep: `ign tags export default` and `ign tags config get default` (provider-ROOT paths) crash the deployed tagConfig route with route_error 'IllegalStateException: No RpcContext available' on BOTH live 8.3.3 gateways; leaf/subtree paths ([default]uat_probe) work fine — system.tag.getConfiguration/exportTags on a provider root need an RpcContext the WebDev thread does not carry"
  severity: major
  test: 12
  root_cause: "system.tag.getConfiguration/exportTags with a provider-ROOT path ([default] alone) internally require an RPC context absent on WebDev Jetty threads (8.3.3 b2026012009); the route passes the IllegalStateException through as route_error. Phase 5 live gates only ever used subtree paths, so the root form was never live-exercised."
  artifacts: ["webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py (line 103 getConfiguration, line 138 exportTags)", "crates/ignition-core/src/actions/tags.rs (provider-root export path composition)"]
  missing: ["Route or CLI-level handling for provider-root forms: either an honest refusal naming the subtree-path form, or a route workaround (root -> recursive folder enumeration via the working leaf machinery); wiremock contracts never catch this (no real gateway semantics)"]
  debug_session: ""
