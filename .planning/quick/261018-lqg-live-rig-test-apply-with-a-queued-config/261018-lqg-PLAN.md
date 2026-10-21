---
phase: quick/261018-lqg
plan: 01
type: execute
wave: 1
depends_on: []
autonomous: true
requirements: [QUICK-lqg, DAPL-06, DAPL-07, DAPL-10]
files_modified:
  - crates/ignition-core/tests/live_deploy_apply.rs
  - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
  - .planning/phases/23-guarded-apply-rollback/23-UAT.md

estimate:
  tokens: 90000
  raw_tokens: 90000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "A live rig test `live_queued_config_save_during_apply` exists in crates/ignition-core/tests/live_deploy_apply.rs, is `#[ignore]`d like the other four gates, and is a quiet green skip unless IGNITION_LIVE_DEPLOY=1"
    - "The test proves the foreign write was QUEUED under the engine's hold: it was sent while GET scan-lock/config named the engine's actor (`api-token:<engine token name>`), it was still unanswered QUEUE_PROBE (3 s) later with the lock still held by the engine, and its response arrived after the hook returned control to the engine (so after the engine's release scan was triggered)"
    - "The foreign write is a signature-checked resource-API PUT to `/data/api/v1/resources/ignition/database-translator` for POSTGRES in the production mode collection: the same route family the gateway UI used in C2b, and a unit the plan writes (changed, over a seed the test created through the API)"
    - "The test passes only when the engine did NOT report `applied` over silently replaced content: either rolled_back with the pre-apply tree back on disk and result.json saying rolled_back, or applied with the deployed config on disk and in REST, the queued write refused, and a fresh deploy verify green"
    - "When the engine reports `applied` while the file or the gateway holds the queued foreign write (or the write was accepted), the test FAILS with a message starting `PHASE 23 GAP:`; engine code is not changed in this task"
    - "The rig is left clean for the other gates: the test sweeps its units and runs one config scan at start and end, and asserts the REST view holds no production translator afterwards"
    - "The new test and the four existing rig gates have been run against ign-deploy-rig (8.3.6); cargo clippy --workspace --all-targets -- -D warnings and cargo fmt --all -- --check are clean"
    - "The result is recorded in 23-LIVE-CAPTURES.md under `## C2b (manual)` and 23-UAT.md test 1 no longer says the check is owed"
  artifacts:
    - "crates/ignition-core/tests/live_deploy_apply.rs: `live_queued_config_save_during_apply`, its helpers (seed/foreign marked configs, on-disk and REST classification, forget-units sweep+scan), the optional IGNITION_LIVE_TOKEN_B writer token, the module doc updated to five gates"
    - ".planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md: a `Rig test live_queued_config_save_during_apply` record appended inside `## C2b (manual)`"
    - ".planning/phases/23-guarded-apply-rollback/23-UAT.md: test 1 result updated (and a Gaps entry if the gap fired)"
  key_links:
    - "FaultyHost `.after(\"commit\", 1, hook)` → the hook runs after the files moved and BEFORE the engine's lock check and release scan; this is what widens the hold window deterministically and orders the foreign write inside it"
    - "Engine `confirm` (crates/ignition-core/src/deploy/apply/mod.rs ~1258): lock check → scan_and_wait (the scan trigger releases the lock, D12) → deploy_verify read_back (compares declared config with the effective REST view). The queued PUT lands about 0.3 s after release (C2b) and races the read_back; the test records which side won"
    - "Seed signature → ResourceUpdate::new(..).with_collection(mode) → queued PUT: the gateway's signature check decides whether the queued write lands (seed signature still current) or is refused as stale (the scan reloaded the deployed copy first)"
---

<objective>
Add the live rig gate `live_queued_config_save_during_apply` (phase 23 UAT item U1, the C2b follow-up) to crates/ignition-core/tests/live_deploy_apply.rs. It shows what `deploy apply` does when a gateway config save to a file the plan writes is queued during the engine's scan-lock hold and lands right after the release. Run it and the four existing rig gates against ign-deploy-rig, then record the result in 23-LIVE-CAPTURES.md and 23-UAT.md.

Purpose: C2b showed that the config scan-lock does not reject a gateway UI/API save. It queues the save (the UI sees 503) and applies it about 0.3 s after `POST /data/api/v1/scan/config` releases the lock. Nobody has checked whether the engine's post-release verify sees such a write, or whether it reports `applied` while the deployed file has been silently replaced. This test answers that with evidence.

Output: one new ignored live test (committed), a recorded live result, and an updated UAT item. If the engine misses the write, the test fails with `PHASE 23 GAP:` and that is reported as a phase 23 gap. Engine code is not changed here.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@crates/ignition-core/tests/live_deploy_apply.rs
@.planning/phases/23-guarded-apply-rollback/23-UAT.md

Read only these line ranges of the large files (do not read them whole):
- .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md lines 377-525 (`## C2`, C2a: token B's POST was queued 59.9 s under A's hold and then answered 200; the body without `collection` went to `core`) and lines 1392-1422 (`## C2b (manual)`, the section this task appends to)
- crates/ignition-core/src/deploy/apply/mod.rs lines 1099-1327 (lock acquire → commit → `confirm`: lock check, scan_and_wait, deploy_verify; green path records post_fingerprint without comparing it to anything)
- crates/ignition-core/src/client/config_resources.rs lines 234-410 (`MutationOutcome`, `ResourceCreate::new(..).with_collection(..)`, `ResourceUpdate::new(name, signature, config)?.with_collection(..)`; prefer `new` over `from_record` per its doc)
- crates/ignition-core/tests/common/faulty_host.rs lines 59-127 (`FaultyHost::new`, `.after(call, nth, hook)`: an `Fn() -> Fut` hook run after invocation `nth` of `call` returned)

Interfaces the executor uses (all already public; verified while planning):
- `ignition_core::client::DeployApi` methods: `resource_create(module, ty, &[ResourceCreate])`, `resource_update(module, ty, &[ResourceUpdate])`, `resource_find(module, ty, name, Some(collection))` → `ResourceRecord { name, collection, signature: Option<String>, config: serde_json::Value, .. }`, `scan_lock_info(ScanKind::Config)` → `LockObservation::{Held(ScanLockInfo { remaining, actor: Option<String>, .. }), Free}`
- `ignition_core::client::deploy_api::scan_and_wait(api, ScanKind::Config, PollConfig)` → `ScanOutcome::{Complete(_), NotComplete{..}}` (`ignition_core::client::ScanOutcome`)
- `ignition_core::client::config_resources::{ResourceCreate, ResourceUpdate, MutationOutcome}`
- `ignition_core::actions::deploy::deploy_verify(&declared, project, deploy, gateway, reader, &VerifyOptions::default())` → `DeployVerifyResult { ok, .. }`
- `deploy_apply_execute(prepared, &ApplyContext, &ApplyOptions)` → `Result<DeployApplyResult, CoreError>`; `DeployApplyResult { outcome: OutcomeKind, ok, cause: Option<RollbackCause>, run_id, run_dir, .. }`
- result.json (crates/ignition-core/src/deploy/apply/mod.rs ~739): the serialized outcome with key `outcome` (`applied` | `rolled_back`), `ok`, `cause`, plus `slug` when rolled back; on an Err it is `{ok:false, run_id, error:{code,message}}`
- Harness already in the test file: `rig()`, `Rig::{api, mode_root, deploy_dir}`, `Clean`, `apply_repo`, `Context`, `collection_state`, `translators_in_mode`, `production_poll`, `gates_for`, `short_hold`, `engine_options`, `utc_now`, `RIG_LOCK`, `TRACER_GATEWAY`, `TRANSLATOR`, `TRANSLATOR_UNIT`
- `crates/ignition-core/src/deploy/apply/hold.rs`: `LOCK_MARGIN_S = 2`, `ACQUIRE_WAIT_S = 1`

Rig facts (checked while planning, 2026-10-18):
- ~/.cache/ign-deploy-rig/tokens.env holds only `IGN_TOKEN_CAP_A` (no second token). Never print or echo any token value.
- The tracer plan writes four files: `ignition/database-translator/POSTGRES/{config.json,resource.json}` and `ignition/general-alarm-settings/{config.json,resource.json}` in `config/resources/<mode>/`. `Clean` sweeps both unit dirs and `ign-deploy/` but does not scan, so the gateway's memory can keep a swept unit until the next config scan.
- The reqwest test client has a 30 s request timeout (crates/ignition-core/src/client/mod.rs `build_client`). A queued PUT must therefore be released within 30 s of being sent, and the hook design below releases it within about 4 s.
- The Designer run of 2026-10-18 21:34Z connected a Designer to this rig. `DesignersPolicy::default()` refuses an apply while a Designer session is open.
</context>

<tasks>

<task type="tracer">
  <name>Task 1: End-to-end queued-save gate: seed over the API, queue a foreign PUT under the engine's hold, judge the verdict</name>
  <files>crates/ignition-core/tests/live_deploy_apply.rs</files>
  <precondition>The ign-deploy-rig gateway answers on http://localhost:18290 with gateway-info name `ign-deploy-rig`, and no Designer session is open on it. If the apply is refused with deploy_preflight_failed for open Designers, stop and ask the user to close the Designer. Do not change DesignersPolicy.</precondition>
  <action>
Add one new `#[tokio::test] #[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]` test named `live_queued_config_save_during_apply` in a new section `// ---- live_queued_config_save_during_apply (C2b, UAT U1) ----` at the end of the file. Reuse the existing harness and do not change the four existing tests' behavior. Do not edit any file under crates/ignition-core/src; this task only records what the engine does.

Harness changes (small, shared):
- `Rig` gets `writer_token: Option<String>`, filled in `rig()` from the optional env var `IGNITION_LIVE_TOKEN_B` (trimmed, empty means None, never printed). Add `Rig::writer_api()`, which builds a `ReqwestGatewayApi::for_tests` with that token, or with `self.token` when it is None. Add `Rig::writer_label()`, returning `"second token (IGNITION_LIVE_TOKEN_B)"` or `"engine token"` for output. Add `Rig::engine_actor_prefix()`, returning `format!("api-token:{}", <text of self.token before the first ':'>)`. That is the token NAME (cap-a), which is not secret. The tokens.env file has only cap-a, so the default run writes with the engine's own token, as the task allows.
- Update the module doc. The first paragraph lists five gates: add "and a gateway config save queued during the scan-lock hold (C2b)". Add the row `IGNITION_LIVE_TOKEN_B` to the env table: optional, a second `name:key` API token for the foreign writer, never printed; absent means the engine's token is used.

Constants and helpers (near the new test):
- `SEED_MARKER = "lqg-seed"`, `FOREIGN_MARKER = "lqg-foreign"`, `MARKED_FIELD = "currentTimeQuery"`, `QUEUE_PROBE = Duration::from_secs(3)`, `PUT_DEADLINE = Duration::from_secs(60)`, `TRANSLATOR_NAME = "POSTGRES"`.
- `rendered_translator_config() -> serde_json::Value`: parse TRACER_GATEWAY's `resources/ignition/database-translator/POSTGRES/config.json`.
- `marked(config, marker) -> serde_json::Value`: a clone with MARKED_FIELD set to the string `SELECT CURRENT_TIMESTAMP /* <marker> */`. It changes a config field that the verify `read_back` compares (deploy_verify reads the effective REST config).
- `enum Seen { Missing, Seed, Foreign, Declared, Other }`, plus `classify(value: &serde_json::Value) -> Seen`. Foreign or Seed means MARKED_FIELD contains the marker. Declared means the value equals `rendered_translator_config()`, with JSON value equality (the API writes its own formatting, so never compare bytes). Other covers the rest.
- `on_disk(rig) -> Seen`: read `rig.mode_root()/ignition/database-translator/POSTGRES/config.json`. Missing if absent, else parse and classify.
- `async fn in_rest(api, mode) -> Seen`: `resource_find("ignition","database-translator",TRANSLATOR_NAME,Some(mode))`. NotFound is Missing, any other Err is Other. Otherwise classify `record.config`.
- `async fn forget_units(rig, api) -> Vec<String>`: build a `Clean` path sweep (the same two unit dirs and `ign-deploy/`), then `scan_and_wait(api, ScanKind::Config, production_poll("config scan to forget the test units"))`, then return `translators_in_mode`. The gateway then forgets any production POSTGRES. This is needed because a bare sweep leaves the unit in gateway memory, and `live_apply_then_rollback` compares REST translators before and after.

Test flow (keep each step's output to counts, markers, codes, run ids and timings, never a token or file bytes):
1. `rig()` (quiet skip), take `RIG_LOCK`, `Clean::new`, then `forget_units` once, so a cut-off earlier run cannot leave a seed in gateway memory. Print the UTC start.
2. `apply_repo`, then `Context::new`. Seed: `resource_create("ignition","database-translator", &[ResourceCreate::new(TRANSLATOR_NAME, marked(rendered, SEED_MARKER)).with_collection(&rig.mode)])` with the engine api. Then `resource_find(.., Some(&rig.mode))`, and expect `collection == Some(mode)`, a non-empty signature, and `in_rest == Seed` and `on_disk == Seed`. Keep the seed signature. If the create is refused, read `resource_names("ignition","database-translator")` to see which collections hold POSTGRES, adjust (for example, a leftover production copy means forget_units did not run), and record the deviation in the SUMMARY. Do NOT seed by hand-writing config.json under the fixture's resource.json: its lastModificationSignature would no longer match.
3. `before = collection_state(&rig)` (includes the seed), `plan_before = ctx.plan_hash()`, and `deploy_apply_prepare` with `gates_for(&plan_before)`. Expect `Ready`, `summary.changed >= 1` (POSTGRES changed over the seed) and `has_changes`. Print the summary counts.
4. Foreign write: `writer = Arc::new(rig.writer_api())`, `update = ResourceUpdate::new(TRANSLATOR_NAME, seed_signature, marked(rendered, FOREIGN_MARKER))?.with_collection(&rig.mode)`.
5. Hold-window probe. Use the user's design (poll the lock until the engine holds it, then write), widened deterministically. A 4-file local commit holds the lock well under a second, so polling from outside cannot reliably land inside it. Add a shared `Arc<std::sync::Mutex<Probe>>` where `Probe` records `actor_at_send: Option<String>`, `sent_at`, `pending_after_probe: Option<bool>`, `actor_after_probe: Option<String>`, `hook_returned_at` (all `Option<Instant>`), and `handle: Option<tokio::task::JoinHandle<(Result<MutationOutcome, CoreError>, Instant)>>`. Wrap the apply host as `FaultyHost::new(LocalFsTransport::new(rig.data_dir.clone(), "nodes.master").with_apply(GATEWAY_DIR, &rig.mode)?).after("commit", 1, hook)`. The hook runs after the files moved and before the engine's lock check and release scan. In the hook:
   (a) read `scan_lock_info(ScanKind::Config)` with the engine api and record the actor, or "free";
   (b) spawn a tokio task that sends `writer.resource_update("ignition","database-translator", &[update.clone()])` and returns `(result, Instant::now())`; record `sent_at`;
   (c) sleep QUEUE_PROBE;
   (d) record `pending_after_probe = Some(!handle.is_finished())`, read the lock again and record its actor;
   (e) store the handle, record `hook_returned_at`, and return.
   The hook must never panic: it only records, as in `live_lock_lapse_restores`. Never hold the std Mutex guard across an `.await`: lock, write, and drop it in a scoped block, because clippy's await_holding_lock would fail the -D warnings gate. The 30 s hold (`short_hold(30)`) leaves far more than LOCK_MARGIN_S after the ~3.5 s hook. That widened window is the documented deviation from pure polling: record it in the SUMMARY and in the captures.
6. Run `deploy_apply_execute(prepared, &ApplyContext{ host: &host, deploy/gateway: engine api, backup_deploy: None, clock: &SystemClock, observer: &NoObserver, .. }, &engine_options(short_hold(30), gates_for(&plan_before)))` and KEEP the `Result` (no expect).
7. Take the handle and `tokio::time::timeout(PUT_DEADLINE, handle)` to get the PUT result and its completion instant. Keep everything as values, not panics.
8. Gather the observations BEFORE any cleanup: `on_disk(&rig)`, `collection_state(&rig)`, and `in_rest(engine api, mode)`. When the run is Ok with a run_dir, parse `result.json` there (its `outcome`, `ok` and `cause`) and collect the `events.ndjson` event names. When the outcome is Applied, run a fresh `deploy_verify(&ctx.declared, "ign-cli", &api, &api, ctx.reader.as_ref(), &VerifyOptions::default())` and keep `ok`. When the outcome is RolledBack, run `Context::new(repo.path()).plan_hash()` (kept as a value; a plan over a leftover journal refuses). Print one findings block: UTC, writer label, actor at send, pending after the probe, the PUT verdict (`accepted`, or the CoreError `code()`), seconds from `hook_returned_at` to PUT completion, run outcome and cause, the result.json outcome, the on-disk and REST classifications, the fresh verify ok, and the event names.
9. Cleanup BEFORE the assertions: `forget_units(&rig, &api)`, and keep the returned translator list. `Clean`'s Drop also sweeps at the end.
10. Assertions, in this order, each with a message naming what was seen:
   (a) Scenario validity. `actor_at_send` starts with `rig.engine_actor_prefix()`. `pending_after_probe == Some(true)`; otherwise fail with "C2 contradicted: the gateway answered the write while the engine held the lock (not queued)". Name the writer label, because an engine-token write that is not queued is itself a finding. `actor_after_probe` also starts with the engine prefix. The PUT completed within PUT_DEADLINE, and its completion instant is after `hook_returned_at`.
   (b) Gap check. The run is Ok with outcome Applied AND any of: the PUT was accepted, `on_disk != Declared`, `in_rest != Declared`, or the fresh verify is not ok. Then `panic!` with a message starting `PHASE 23 GAP:`. Name the run id, the on-disk and REST classifications, and the PUT verdict, and say that deploy apply reported applied while the deployed POSTGRES config was replaced by the queued foreign save.
   (c) Accepted branch, RolledBack: `result.cause.is_some()` (print it; verify_failed is the expected one); `collection_state` equals `before`, the seed tree back; result.json `outcome == "rolled_back"` and `ok == false`; the post-run plan equals `plan_before`; the events include `restore_started`.
   (d) Accepted branch, Applied (only reachable when (b) did not fire): the PUT was refused (print its code; resource_signature_rejected is the expected one), `on_disk == Declared`, `in_rest == Declared`, the fresh verify is ok, and result.json `outcome == "applied"`.
   (e) Any other result (Ok Noop, or an Err from the engine) fails with a message carrying the outcome or the error `code()`. The executor records it as a finding.
   (f) The translator list from cleanup is empty: the rig is clean for the other gates.
   End with `eprintln!("PASS live_queued_config_save_during_apply in {:.1}s ({branch})", ..)`, where branch is `rolled_back` or `applied_write_refused`.

Then compile and run it against the rig. Iterate only on the request shape and harness (for example the seed create, `with_collection`, the probe timing). If the probe shows the write is not queued with the engine token, that is a recorded finding. To still exercise the scenario, mint a second token with `bash crates/ignition-core/tests/fixtures/live-rig-deploy/mint-token.sh cap-a cap-b`. This rewrites tokens.env with new keys for both and restarts the gateway. Wait until the rig is healthy (gateway-info answers), source tokens.env again, rerun with `IGNITION_LIVE_TOKEN_B="$IGN_TOKEN_CAP_B"`, and record both runs. Run cargo fmt on the file and commit it as `test(23): live gate for a config save queued during the apply hold`, with no co-author trailer and no push. Commit it even when the gate ends in `PHASE 23 GAP:`, since that failure is the deliverable evidence.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && cargo test -p ignition-core --test live_deploy_apply 2>&1 | tail -3 && test "$(cargo test -p ignition-core --test live_deploy_apply -- --list --ignored 2>/dev/null | grep -c 'live_queued_config_save_during_apply')" = 1 && . ~/.cache/ign-deploy-rig/tokens.env && IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data" IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_apply live_queued_config_save_during_apply -- --ignored --nocapture --test-threads=1 > "${TMPDIR:-/tmp}/lqg-tracer.log" 2>&1; grep -E '^PASS live_queued_config_save_during_apply|PHASE 23 GAP:' "${TMPDIR:-/tmp}/lqg-tracer.log" && test "$(grep -cF "${IGN_TOKEN_CAP_A#*:}" "${TMPDIR:-/tmp}/lqg-tracer.log")" = 0</automated>
  </verify>
  <done>The test compiles and is a quiet skip with the gate closed. Against the rig it reaches a verdict with the queue proof established (actor at send = the engine's, pending after the probe, completed after the hook returned). It ends in `PASS ... (rolled_back)`, `PASS ... (applied_write_refused)` or `PHASE 23 GAP:`, and the log carries no token. The test file is committed.</done>
</task>

<task type="auto">
  <name>Task 2: Run all five rig gates sequentially, plus clippy and fmt</name>
  <files>crates/ignition-core/tests/live_deploy_apply.rs</files>
  <precondition>Task 1's commit exists and the rig is still healthy with no Designer session open.</precondition>
  <action>
Run the full ignored suite once against the rig with the exact rig env from the task: source ~/.cache/ign-deploy-rig/tokens.env, set `IGNITION_LIVE_TOKEN_B` only if Task 1 had to mint cap-b, and use `--test-threads=1`. Capture the log to `${TMPDIR:-/tmp}/lqg-all.log`. The four existing gates must each print their `PASS <name>` line. The new gate must print its PASS line or fail with `PHASE 23 GAP:`. If an existing gate fails, use superpowers:systematic-debugging. First suspect rig state the new test left behind (a production POSTGRES in gateway memory, a held lock, a journal under ign-deploy/), then the shared-CARGO_TARGET_DIR symptoms in .claude/CLAUDE.md. Fix only the test harness; never engine code. Then run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`, and fix any finding in the test file. Commit only if something changed (`test(23): ...`, no co-author, no push). Note each gate's wall time from the log for the record.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && . ~/.cache/ign-deploy-rig/tokens.env && IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data" IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1 > "${TMPDIR:-/tmp}/lqg-all.log" 2>&1; test "$(grep -cE '^PASS (live_apply_then_rollback|live_kill_after_green_verify_then_recover|live_kill_mid_write_then_recover|live_lock_lapse_restores) in' "${TMPDIR:-/tmp}/lqg-all.log")" = 4 && grep -E '^PASS live_queued_config_save_during_apply|PHASE 23 GAP:' "${TMPDIR:-/tmp}/lqg-all.log" && grep -E '^test result:' "${TMPDIR:-/tmp}/lqg-all.log" && test "$(grep -cF "${IGN_TOKEN_CAP_A#*:}" "${TMPDIR:-/tmp}/lqg-all.log")" = 0 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check && git status --short crates/</automated>
  </verify>
  <done>The four existing gates PASS in the same run as the new gate. The new gate PASSes or fails only with `PHASE 23 GAP:`. Clippy and fmt are clean, and nothing under crates/ is left uncommitted.</done>
</task>

<task type="auto">
  <name>Task 3: Record the result in 23-LIVE-CAPTURES.md (C2b) and 23-UAT.md (test 1)</name>
  <files>.planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md, .planning/phases/23-guarded-apply-rollback/23-UAT.md</files>
  <action>
Use Edit (scoped), never Write, on both files. Write plain, factual prose in the files' existing voice, with no tokens and no file bytes.

23-LIVE-CAPTURES.md: inside `## C2b (manual)`, after the Designer run's Conclusion paragraph and before `## C11`, append a paragraph group headed `Rig test live_queued_config_save_during_apply, <UTC date and time span from the log>`. It covers:
- the setup (8.3.6 rig, seed POSTGRES translator created in production over the resource API, plan = POSTGRES changed plus alarm settings added);
- the writer actor (engine token cap-a, or cap-b if minted) and the route (`PUT /data/api/v1/resources/ignition/database-translator`, collection production, the seed signature);
- how the hold window was widened (a hook after the commit, before the engine's lock check and release scan, 3 s probe, hold 30 s), and why: a 4-file local commit holds the lock for well under a second;
- the queue proof (actor at send, pending after the probe, seconds from the hook return to the PUT answer);
- the PUT verdict, the run outcome and cause, the result.json outcome, and the final on-disk and REST classification of POSTGRES config.json;
- the cleanup (sweep plus one config scan, no production translator left);
- the five-gate run result from Task 2.
Then write a Conclusion line that states plainly one of the following. The engine detects a queued config save and rolls back. Or the queued save is refused as stale because the scan reloaded first, and the deployed file stays. Or GAP: the engine reported applied while the file held the foreign save. Also replace the sentence in the Designer conclusion that says the question "is owed to the live rig test `live_queued_config_save_during_apply`, a quick task" with a pointer to the new record (for example "is answered by the rig test record below").

23-UAT.md: in test 1, replace the trailing sentence that marks the queued-save check as outstanding (the one that points at a live rig test in a quick task) with the rig test's outcome in one or two sentences (UTC, outcome and branch, pointer to the C2b record). Set frontmatter `updated:` to the current UTC time.
- If the gap fired, change test 1 `result:` to `issue`. Add an entry under `## Gaps`: a `- truth:` line ("An apply never reports applied while a config save queued during its scan-lock hold has replaced a deployed file"), `status: failed`, `reason:` (the observed outcome, on-disk and REST classification, run id), `severity: major` and `test: 1`. In `## Summary`, set `passed: 1` and `issues: 1`.
- Otherwise keep `result: pass` and leave the Summary counts as they are.

Commit both files as `docs(23): record the queued config save rig test (C2b, U1)`, with no co-author trailer and no push.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && awk '/^## C2b \(manual\)/{f=1} /^## C11/{f=0} f' .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md | grep -c 'Rig test live_queued_config_save_during_apply' && test "$(awk '/^### 1\. C2b/{f=1} /^### 2\./{f=0} f' .planning/phases/23-guarded-apply-rollback/23-UAT.md | grep -c 'Still owed')" = 0 && awk '/^### 1\. C2b/{f=1} /^### 2\./{f=0} f' .planning/phases/23-guarded-apply-rollback/23-UAT.md | grep -c 'live_queued_config_save_during_apply' && . ~/.cache/ign-deploy-rig/tokens.env && test "$(grep -cF "${IGN_TOKEN_CAP_A#*:}" .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md .planning/phases/23-guarded-apply-rollback/23-UAT.md | awk -F: '{s+=$NF} END{print s}')" = 0 && git log -1 --format=%s | grep -c 'queued config save'</automated>
  </verify>
  <done>`## C2b (manual)` holds the rig test record with a plain conclusion. UAT test 1 states the outcome and no longer says the check is owed, and if the gap fired it is listed under `## Gaps` with the Summary updated. Neither file contains a token, and the docs commit exists.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| test process → rig gateway REST | Live writes (resource create/update, scan-lock, scan) cross into a real gateway. They must reach only the disposable loopback rig |
| tokens.env → test env → logs/docs | API token values enter the process via env. Output, logs and planning docs must never carry them |
| new test → shared rig state | The five gates share one gateway. A unit or lock left behind by one test corrupts the next |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-lqg-01 | Information disclosure | test output, run logs, 23-LIVE-CAPTURES.md, 23-UAT.md | high | mitigate | Tokens are read only from env (`IGNITION_LIVE_TOKEN`, optional `IGNITION_LIVE_TOKEN_B`) and never formatted. Only the token NAME prefix (`api-token:cap-a`) is used for the actor check. Every verify step greps the log and docs for the key part of `$IGN_TOKEN_CAP_A` and requires 0 hits |
| T-lqg-02 | Spoofing / Tampering | `rig()` before the seed write | high | mitigate | The existing `local_url_guard` refuses non-loopback URLs, and the gateway-info name guard (`ign-deploy-rig`) runs before the first write. The new test calls `rig()` first and writes nothing before it returns |
| T-lqg-03 | Tampering | production POSTGRES seed/foreign copy in gateway memory | medium | mitigate | `forget_units` (sweep plus one config scan) runs at start and before the assertions. Assertion (f) requires no production translator afterwards, and Task 2 runs all five gates together to prove the rig is left clean |
| T-lqg-04 | Denial of service | engine scan-lock left held after a panic mid-run | low | accept | `short_hold(30)` lets an orphaned hold lapse in 30 s. The hook never panics, and engine results are kept as values until after cleanup |
| T-lqg-05 | Tampering | engine code | medium | mitigate | Scope is test-only. No file under crates/ignition-core/src changes, and a gap is recorded, not fixed, per the task |

No new dependencies are installed (reqwest, tokio and serde_json are already crate deps), so there is no supply-chain row.
</threat_model>

<verification>
- `cargo test -p ignition-core --test live_deploy_apply` (gate closed) is green, and `--list --ignored` shows five live gates.
- One sequential live run shows the four existing PASS lines and the new gate's PASS line or `PHASE 23 GAP:` failure, and its log has no token.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` are clean.
- `git diff --stat HEAD~2 -- crates/ignition-core/src` is empty (no engine change). Commits carry no co-author trailer, nothing is pushed, and no worktree was created.
- 23-LIVE-CAPTURES.md `## C2b (manual)` and 23-UAT.md test 1 hold the result.
</verification>

<success_criteria>
- The question left open by C2b ("does an apply detect a config save queued during its hold?") has a recorded, evidence-backed answer from a real 8.3.6 gateway.
- If the answer is a gap, it is visible as a failing `PHASE 23 GAP:` gate and a UAT Gaps entry, and it is not papered over.
- The four existing rig gates still pass alongside the new one.
</success_criteria>

<output>
Create `.planning/quick/261018-lqg-live-rig-test-apply-with-a-queued-config/261018-lqg-SUMMARY.md` when done. Include the branch the gate took, the queue-proof timings, the writer actor, any deviations (the hook-widened window, a minted cap-b, a seed-create adjustment), and the five-gate run result.
</output>
