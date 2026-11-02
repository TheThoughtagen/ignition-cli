---
status: resolved
trigger: "ign deploy apply of a tag-definition unit wedges Ignition 8.3.6 redundancy config-resource sync"
created: 2026-11-01
updated: 2026-11-02T06:10Z
---

# Debug: deploy-tag-redundancy-sync

## Trigger

DATA_START
ign deploy apply of a tag-definition unit wedges Ignition 8.3.6 redundancy config-resource sync. Reproduce on the Docker 8.3.6 pair rig, find the cause, and decide the fix.
DATA_END

## Symptoms

- **Expected:** `ign deploy apply` writes a tag-definition unit on the master only. Ignition redundancy syncs it to the backup, and apply ends `applied` with the backup converged (as it does for non-tag units on the Phase 24 pair rig).
- **Actual:** the master loads the unit and verify is green. The backup never receives it. The backup's next `[config-resource]` sync fails with `RedundancySyncException: [config-resource] After applying diff, new local system snapshot '<X>' did not match remote system snapshot '<Y>'. A full pull must be executed`. Then come 5-6 failed full pulls and "No further synchronization attempts will be made", and the backup goes to `projectState: SyncError`. The `tag-config-resource` provider did not error. Apply ends `applied_backup_unsynced` (`files` check: `file_missing_on_backup` for both files).
- **Errors:** as above. The stack is `AbstractResourceSynchronizer.applyState:295` <- `ConfigResourceSynchronizer.applyPartialState:195` (first) / `applyFullState:200` (retries). The master logs no error, only "Remote needs to do full pull".
- **Timeline:** the production pair(Ignition 8.3.6, deployment mode `production`) synced fine through 2026-10-29. The first failure was 2026-10-30T21:54:40Z, 2 s after the first canary apply's scan (run 20261030T213754Z-28f01c). It reproduced on 2026-11-01 (run 20261101T214756Z-7a27e5) right after a clean master restart.
- **Reproduction (prod, do NOT repeat on prod):**
  1. apply stages via sftp;
  2. takes the config scan-lock (`POST /data/api/v1/scan-lock/config`);
  3. commits `config/resources/production/ignition/tag-definition/default/Deploy/{tags.json,unary-resource.json}`;
  4. calls `POST /data/api/v1/scan/config`.

  The unit: `tags.json` holds one memory String tag, `[{"valueSource":"memory","dataType":"String","name":"Canary","defaultValue":"ign v1.6.2","tagType":"AtomicTag"}]`. `unary-resource.json` is `{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["tags.json"],"attributes":{"config":{}}}`.
- **Recovery observations:**
  - `ign deploy rollback` followed by a backup restart does NOT clear it. The same two snapshot hashes (local `7f8bea…`, remote `8a34c7…`) persisted for 2 days, with the canary both present and absent.
  - Rollback followed by a master `Restart-Service Ignition` clears it: both nodes return to Good.
  - Re-applying after the clean restart re-broke it within one sync, with new hashes `e094c63d`/`6fa85140`.
  - A master restart also makes the gateway rewrite `tag-provider/MQTT Transmission` and `NextTrend-NextTrend` (the `nonUseCount` values and the NextTrend uuid).

## Prior coverage

Phase 24 pair rig:
- `.planning/milestones/v1.6-phases/24-redundant-pair/24-LIVE-CAPTURES.md`
- `crates/ignition-core/tests/live_deploy_pair.rs`
- `crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh`
- data dirs `~/.cache/ign-pair-rig/data-a|b`

It only applied non-tag units (database-connection, database-translator, oauth2-client), and they converged. No tag-definition unit was ever applied to a pair. See the K2 finding in that doc: a resource.json with `lastModification` but no valid signature never converges on the backup.

## Goal

1. **Reproduce on the pair rig.** Apply the same canary tag unit into `production` the way apply does (files, scan-lock, scan/config), and confirm the config-resource snapshot mismatch in gw-b's logs. Controls:
   - the same tag unit into `core`;
   - a non-tag unit into `production`;
   - the tag created through the gateway's own tag config path (REST or tags import) instead of files.
2. **Identify the cause:** which resource, collection or shape makes the master's snapshot unreproducible, and whether the master's in-memory snapshot goes stale on file + scan.
3. **Recommend the fix for ign**, for example writing tag units through a sync-safe gateway API, or a different file shape or collection. Back it with a live pair gate that proves convergence.

## Constraints

- No production calls at all. Use the pair rig and Docker only.
- No git worktrees (shared CARGO_TARGET_DIR). Work on branch `fix/deploy-tag-redundancy-sync` in the main checkout.
- Never commit `.planning/config.json`, `.planning/research/.cache/` or `svc-ignition-deploy@52.154.75ace-uat4.ps1`.
- No co-author lines.

## Current Focus

- bug_class: Bohrbug (deterministic, reproduced 6 of 6 times on the rig: T1, T6, C4, C5 wedge; C1, C2, C3, C6 converge)
- decision (user, 2026-11-02): "C now, then A". Option C ships in this session as a patch release; Option A (tag units into `core`) is the next step, NOT implemented here.
- status: fixing (option C).
- fix plan (C):
  1. New stable slug `deploy_pair_tags_refused` (exit 6) under the four-place rule (variant + text, code(), exit-6 arm, hint, DEPLOY_SLUGS + pins, exit_code_mapping_enumerated, EXIT_SLUG_LITERALS, module doc row, four-place test; README exit table row 6, apply order list, output-contract list, apply exit-code paragraph; deploy_family_coverage count 16 -> 17).
  2. Gate (pure, writes nothing): for a target with `nodes.backup`, any plan entry of `ignition/tag-definition` or `ignition/tag-type-definition` that is `added` or `changed` (the apply would write it into the mode collection) or `same` with a live copy in the mode collection (the apply would keep it there) is refused, after `plan_stale`/`deploy_unreadable` and before the no-op return. `removed` tag entries stay allowed (deleting them is the recovery). Engine double guard in `run_apply` (WR-04 pattern). `deploy plan` warns on a pair target. `deploy rollback` on a pair refuses a restore that would put a tag unit back into the mode collection (normal rollback only; interrupted-run recovery is not refused).
  3. Cleanup: the emptied `ignition/tag-definition` / `ignition/tag-type-definition` type folder (depth 2) is removed by commit and restore on every host (local apply_ops remove_emptied, commit.sh rmdir_up, commit.ps1 RmdirUp, the sftp script-report parsers, and the test-only mirror for parity). Other types keep the `<module>/<type>` floor.
  4. Files check stays authoritative (projectState is already "recorded, not a condition", V9): pin it with a unit test (projectState Good + files fail = not converged).
  5. Live pair gates on the 8.3.6 rig: refusal fires with nothing written; a wedged pair (tag unit written by a master-only target, the pre-fix shape) is recovered by an apply that deletes it (type folder removed), the backup converges, and a following non-tag change reaches gw-b.
  6. README runbook T6e for production recovery.
- progress: steps 1-6 done and committed on fix/deploy-tag-redundancy-sync (a9635c4 cleanup, a977786 refusal, bebc877 README + runbook, 894672b live gates + pair test). All gates green; guardrail accepted; rig torn down.
- next_action: human verification (checkpoint): review the four commits and the runbook, decide whether and when to run runbook T6e on the production pair, then reply "confirmed fixed" or what is still failing. Option A (tags into core) is the recorded next step.

```yaml
reasoning_checkpoint:
  hypothesis: "Ignition 8.3.6's config-resource synchronizer counts tag-definition and tag-type-definition resources of every MODE collection in its system snapshot (ConfigurationManager.find(mode), no filter) but excludes them from every diff (TAG_EXCLUDE_FILTER), and tag-config-resource only syncs `core`. ign deploy writes tag units into the mode collection (`production/ignition/tag-definition/...`), so the master's digest can never be reproduced on the backup: the backup fails 5 full pulls and stops syncing config-resource entirely."
  confirming_evidence:
    - "Bytecode: ConfigResourceSynchronizer.getSnapshot uses find(\"core\", TAG_EXCLUDE_FILTER) but find(mode) for each mode; AbstractResourceSynchronizer.pull uses ResourceCollectionManager.pull(..., TAG_EXCLUDE_FILTER); TagConfigSynchronizer reads `core` only."
    - "T1: the exact production canary sequence on the 8.3.6 pair rig reproduces the production log chain (same stack, `diff=[]`, 5 failures, SyncError, files missing on gw-b)."
    - "C1: the identical bytes in `core` converge (tag-config-resource carries create and delete). C3: the gateway's own tag import writes `core` even in production mode and converges. C2/C6: non-tag and tag-group units in `production` converge."
    - "C4: a matching copy on the backup is DELETED by the master's diff (DeleteResourceOperation ... collectionName=production) and the digests still differ. C5: tag-type-definition in production wedges the same way."
    - "T6: an EMPTY `production/ignition/tag-definition/` folder alone wedges the pair; T1c/T1d and T6e: removing it (scan + resync, or + backup restart) is what recovers."
  falsification_test: "A tag-definition unit in a mode collection that converges on an 8.3.6 pair, or a core tag unit that wedges it. Neither happened in 10 live runs."
  fix_rationale: "Every fix must keep tag-definition / tag-type-definition paths (files AND the empty type folder) out of the master's mode collection, because the mismatch is in Ignition and cannot be repaired from the backup side (C4). Routing those units to `core` (A) or letting the gateway write them (B) puts them where tag-config-resource syncs them; refusing them (C) stops the wedge."
  blind_spots: "Ignition versions other than 8.3.6 (the filter may be fixed later); Windows/sftp transport (production) leaves the same empty type folder? (non-unix commit_apply also stops at depth 3, not verified live here); whether the production backup is still silently unsynced right now (T6b says it can be, while both nodes read Good); core-collection ownership effects of option A on gateways where Designer users also edit the same tag folders."
  candidate_causes:
    - "code (Ignition 8.3.6): snapshot vs diff filter mismatch for mode collections; tag-config-resource core-only (CONFIRMED)"
    - "code (ign): deploy places tag units in the mode collection, and rollback leaves `<mode>/ignition/tag-definition/` behind (CONFIRMED contributing)"
    - "data (unit shape: one-line JSON, `attributes.config` marker, folder unit): ELIMINATED by C1"
    - "environment (mode activation order / restart history, the spike's RIG.md path): ELIMINATED, the fresh correctly-ordered rig wedges only on tag paths in production"
  and_gate: "yes. The wedge needs BOTH the Ignition filter inconsistency AND a tag-definition / tag-type-definition path in a mode collection on the master (ign's layout choice). Persistence after rollback needs a third condition: the leftover empty type folder (ign's delete cleanup stops at <module>/<type>), and Ignition never retries an exhausted config-resource sync without a resync or a backup restart."
```

## Evidence

- timestamp: 2026-11-02T03:15Z
  checked: bytecode (javap -c) of `gateway-8.3.6.jar` copied out of the rig's gw-a container: `AbstractResourceSynchronizer`, `ConfigResourceSynchronizer`, `TagConfigSynchronizer` (package `com.inductiveautomation.ignition.gateway.redundancy.state.sync.providers` and `...tags.config.storage.resource.valueStore`).
  found: |
    - `AbstractResourceSynchronizer.TAG_EXCLUDE_FILTER` = `ResourceFilter.newBuilder().addResourceType(TagResourceTypes.TAG_DEFINITION).addResourceType(TagResourceTypes.TYPE_DEFINITION).setExcludeMode(true).build()`.
    - `AbstractResourceSynchronizer.pull(...)` (used by both `ConfigResourceSynchronizer.pullPartial` and `pullFull`) builds the diff with `ResourceCollectionManager.pull(snapshots, TAG_EXCLUDE_FILTER)` for every collection, and stamps it with `RedundancyResourceSnapshot.calculateSnapshotDigest` of `getSnapshot(...)`.
    - `ConfigResourceSynchronizer.getSnapshot(boolean)`: `core` is read with `ConfigurationManager.find("core", TAG_EXCLUDE_FILTER)`, but each mode from `ConfigurationManager.getModes()` is read with `ConfigurationManager.find(mode)`, NO filter. So tag-definition / tag-type-definition resources in a mode collection count in the snapshot digest but are filtered out of every diff.
    - `AbstractResourceSynchronizer.applyState(...)` on the backup: `forcePush` of the diff's operations, then `getSnapshot(...)` and compare digests; mismatch -> `RedundancySyncException` "[config-resource] After applying diff, new local system snapshot ... did not match remote system snapshot ...".
    - `TagConfigSynchronizer` (`tag-config-resource`): both `getSnapshot` and `pullInternal` use the `core` collection only, with `TAG_CONFIG_FILTER`. Mode collections are never synced for tag types.
  implication: a tag-definition unit committed into the `production` collection can never reach the backup through either synchronizer, and its presence alone makes the master's `config-resource` digest unreproducible. Explains `file_missing_on_backup` for both files and the snapshot mismatch with `tag-config-resource` healthy. Needs live confirmation on the rig.

- timestamp: 2026-11-02T03:18Z
  checked: |
    T1 (repro) on the pair rig (fresh `setup-pair.sh up/mint/pair/mode`, settled 03:16:49Z: gw-a Master Active Good production, gw-b Backup Cold Good production). Loggers raised on both nodes: `Redundancy.StateMonitoring.SyncProviders.ConfigResource` TRACE, `tags.storage.resource.RedundantSynchronizer` TRACE, `Redundancy.StateMonitoring.SyncManager` DEBUG. At 03:18:03Z, the apply sequence by hand on gw-a: `POST scan-lock/config` (token pair-a, held), wrote `production/ignition/tag-definition/default/Deploy/{tags.json,unary-resource.json}` with the exact canary bytes from Symptoms, `POST scan/config`, waited for completion (03:18:04.869).
  found: |
    gw-a 03:18:05.287 `Internal resource collection 'production' updated` -> config-resource and tag-config-resource both flagged changes.
    gw-a 03:18:06.511 T `[config-resource] Providing redundant diff. Local snapshot='8f8b0e8b...', diff=[CreateResourceOperation{resourceId=ResourceId{resourcePath=ignition, collectionName=production}}]` (the tag-definition resources are not in the diff; only the bare `ignition` folder).
    gw-a 03:18:06.848 `[tag-config-resource] Pull called. Returning 0 tag diff(s)`.
    gw-b 03:18:06.555 T `Applying redundancy diff. changeOps=[CreateResourceOperation{... resourcePath=ignition, collectionName=production}]`, then 03:18:06.649 `RedundancySyncException: [config-resource] After applying diff, new local system snapshot 'e1fe9445...' did not match remote system snapshot '8f8b0e8b...'. A full pull must be executed`.
    Four full pulls follow; each time the master logs `Providing redundant diff ... diff=[]` with the same `8f8b0e8b...` and gw-b computes `e1fe9445...`. 03:18:15.745 E `Sync operation has failed 5 time(s) ... No further synchronization attempts will be made`.
    gw-b `tag-config-resource` 03:18:06.920 `Diff successfully applied` (changeOps=[]), no error.
    After 20 s: gw-a and gw-b both `projectState: SyncError`; gw-b `production/` holds only `config-mode.json` (both canary files missing).
  implication: the production symptom reproduces exactly on the 8.3.6 pair rig, with the same stack (`applyState:295` <- `applyPartialState`/`applyFullState:200`), and the TRACE lines show the mechanism predicted from the bytecode: the master's digest counts the tag resources, the diff (partial and full) never carries them. Logs saved in the scratchpad (t1-gwa.log, t1-gwb.log).

- timestamp: 2026-11-02T03:19Z-03:21Z
  checked: |
    T1b, the rollback shape by hand on gw-a at 03:19:11Z: scan-lock, delete `Deploy/tags.json` and `Deploy/unary-resource.json`, rmdir `Deploy` and `default` (what `local.rs` commit_apply removes; `production/ignition/tag-definition/` stays, MIRROR_MIN_COMPONENTS = 3), scan/config. Then T1c: `docker restart` of gw-b at 03:19:51Z (canary absent on the master), loggers raised again.
  found: |
    T1b: gw-a flagged `production` updated (03:19:12.464); gw-b answered `[config-resource] System has determined that this node needs to run a full pull ... But a full pull is already in progress after being triggered by version ...`, i.e. the exhausted config-resource sync is never retried by a new master change. Both nodes stayed SyncError.
    T1c: after the gw-b restart the full pulls ran again and failed again: master `Providing redundant diff. Local snapshot='c925a9eb...', diff=[]`, gw-b `new local system snapshot '93c63e3a...' did not match remote system snapshot 'c925a9eb...'`, five attempts, `No further synchronization attempts will be made` at 03:21:29.083. Both nodes SyncError.
  implication: matches the production recovery observation (rollback + backup restart does not clear it). The master's digest still counts something the diff never carries even with the canary files gone. Two candidates: the empty `production/ignition/tag-definition/` folder left by the rollback (a tag-type folder in a mode collection, still counted, still filtered), or master in-memory staleness (production: only a master restart cleared it).

- timestamp: 2026-11-02T03:22Z-03:36Z
  checked: |
    T1d (differential against T1c, gw-a NOT restarted in either): 03:22:15Z on gw-a scan-lock, `rmdir production/ignition/tag-definition` (the empty folder the rollback leaves), scan/config, then `docker restart` of gw-b.
  found: |
    gw-a 03:23:19.776 `Providing redundant diff. Local snapshot='3dcb2b5d...'`; gw-b 03:23:19.934 `[config-resource] Diff successfully applied. Synchronized snapshot manifest='3dcb2b5d...'`. The digests matched for the first time since T1.
    Rig artifact, not part of the bug: the successful sync renamed gw-b's system id (`_0:2:ign-pair-b` -> `_0:2:ign-pair-a`, the `-n ign-pair-b` start argument vs the master's synced name) and restarted its GAN channel, which did not come back (`setup-pair.sh link` timed out at 300 s, gw-b went Active at 03:25:19). One more gw-b restart + `setup-pair.sh link` (03:35:04Z-03:36:27Z) relinked in 20 s; at 03:36:49Z gw-a Master Active Good, gw-b Backup Cold Good, syncPending false.
  implication: with the canary files gone, the leftover EMPTY `production/ignition/tag-definition/` folder alone keeps the master's digest unreproducible (T1c failed with it, T1d passed without it, same master process). So the trigger is any tag-definition path in a mode collection, even an empty type folder; master in-memory staleness is not needed to explain the rig. A rollback that leaves `<mode>/ignition/tag-definition/` behind does not recover the pair.

- timestamp: 2026-11-02T03:37Z-03:40Z
  checked: |
    Controls on the settled pair (loggers raised again on both nodes).
    C1 03:37:23Z: the SAME canary unit (same bytes, one-line JSON) written into gw-a `core/ignition/tag-definition/default/Deploy/` under the scan-lock, scan/config. Then its delete (files + Deploy dir) and a scan at 03:37:58Z.
    C2 03:38:29Z-03:39:09Z: the existing live gate `live_pair_converges` (the real apply engine; tracer units database-translator/POSTGRES + general-alarm-settings into `production/`; gw-b read only through MethodTap).
    C3 03:39:18Z: the canary created through the gateway's own route on gw-a, `POST /data/api/v1/tags/import?provider=default&type=json&collisionPolicy=Abort` with `{"tags":[{"name":"Deploy","tagType":"Folder","tags":[<canary>]}]}` (gw-a runs deploymentMode production). Cleanup by core file delete + scan at 03:40Z.
  found: |
    C1: gw-b `[tag-config-resource] Applying redundancy tag config diff. changeOps=[CreateResourceOperation{resourcePath=ignition/tag-definition/default/Deploy, collectionName=core}, ...Deploy/_unary...]` -> `Diff successfully applied`; config-resource `changeOps=[]` -> `Diff successfully applied`. Both nodes Good, syncPending false. gw-b `core/.../Deploy/tags.json` byte-equal to gw-a's; `unary-resource.json` equal as parsed JSON (gw-b pretty-prints it). Tag export on both nodes shows `Canary`. The delete propagated the same way (`DeleteResourceOperation ... collectionName=core`), gw-b dropped Deploy/, both Good.
    C2: PASS. `apply: outcome Applied ok true ... backup converged true ... files_checked 4`, checks Redundancy/Mode/ReadBack/Files Pass, `verify --node both` ok, tap GET 15 only, cleanup settled after 0 s.
    C3: `{"successCount":2,"failureCount":0}`. The gateway wrote the folder to `core/ignition/tag-definition/default/Deploy/{tags.json,unary-resource.json}` (NOT to `production/`, although the gateway runs mode production), pretty-printed. tag-config-resource carried it (`CreateResourceOperation ... collectionName=core`); gw-b files byte-equal; both Good.
  implication: the failure is specific to tag-definition (and by the same filter tag-type-definition) resources in a MODE collection. The same bytes in `core` converge, non-tag units in `production` converge, and the gateway itself never puts tags in a mode collection: its native tag write targets `core` even in production mode. ign's choice of `<mode>/ignition/tag-definition/...` for tag units is the path Ignition's redundancy sync cannot carry.

- timestamp: 2026-11-02T03:40Z-03:45Z
  checked: |
    T6 03:40:55Z: on the converged pair, ONLY an empty directory `production/ignition/tag-definition/` created on gw-a (no files), scan-lock + scan/config.
    T6b 03:41:28Z: `docker restart` of gw-a (the master) with that empty directory still present (the production recovery step).
    T6c 03:43:07Z: probe after the master restart: the tracer's `database-translator/POSTGRES` unit written into gw-a `production/` + scan.
    T6d 03:43:53Z: `POST /data/api/v1/redundancy/gwaction/resync` on gw-a (what `ign deploy apply` sends as its one resync), empty directory still present.
    T6e 03:44:30Z: `rmdir production/ignition/tag-definition` on gw-a + scan, then the same resync. 03:45Z: probe unit removed + scan.
  found: |
    T6: wedged exactly like T1: master `Providing redundant diff. Local snapshot='6e34d918...', diff=[]`, gw-b `new local system snapshot 'fae76441...' did not match`, 5 failures, `No further synchronization attempts`, both nodes SyncError. An empty tag-definition type folder in a mode collection is sufficient.
    T6b: after the master restart BOTH nodes report `projectState: Good` (gw-b Cold, syncPending false, lastSyncTimestamp advancing), but gw-b's config-resource provider never ran: `[config-resource] System has determined that this node needs to run a full pull ... But a full pull is already in progress`.
    T6c: the probe unit did NOT reach gw-b (`production/ignition/database-translator/` empty on gw-b) while both nodes stayed `Good`. False green.
    T6d: the resync reset the backup's stuck provider for one attempt: gw-b received POSTGRES (`changeOps=[CreateResourceOperation{... database-translator/POSTGRES, collectionName=production}]`) but failed again (`68109e45...` vs `284ae944...`, `failed 6 time(s)`), both nodes still `Good`. The restarted master still counts the empty folder.
    T6e: after removing the folder + scan + resync: gw-b 03:44:38.321 `[config-resource] Diff successfully applied. Synchronized snapshot manifest='68109e45...'` (gw-a's new digest equals gw-b's previous local one: the empty folder was the only difference). The probe delete then propagated (03:45:06 `Diff successfully applied ... 'fae76441...'`, the pre-T6 digest).
  implication: |
    (1) Any tag-definition path in a mode collection on the master, even an empty type folder, wedges config-resource. (2) A master restart does NOT fix it on the rig; it only turns `projectState` back to `Good` while the backup's config-resource provider stays stuck and silently stops carrying ALL config changes (non-tag ones too). The production report "rollback + master restart clears it: both nodes Good" is therefore not proof of convergence; the production backup may still be silently unsynced, and the 2026-11-01 "new hashes" fit a resync (apply's one resync) restarting the attempts. (3) Working recovery without any restart: remove every tag-definition / tag-type-definition path (including the empty type folder) from the master's mode collection, scan, `POST redundancy/gwaction/resync` on the master; confirm with file comparison (ign's `files` check), not `projectState`.

- timestamp: 2026-11-02T03:45Z-03:47Z
  checked: |
    C4 (probe of a "write both nodes" fix): 03:45:47Z the canary unit written into gw-b `production/` + scan on gw-b, then the same unit into gw-a `production/` under the scan-lock + scan. Recovery 03:46:35Z: `rm -r production/ignition/tag-definition` on gw-a, scan, resync on gw-a.
  found: |
    gw-b 03:45:50.114 T `Applying redundancy diff. changeOps=[DeleteResourceOperation{ignition/tag-definition/default/Deploy/_unary, production}, DeleteResourceOperation{ignition/tag-definition/default/Deploy, production}, DeleteResourceOperation{ignition/tag-definition/default, production}, DeleteResourceOperation{ignition/tag-definition, production}]`: the master's diff DELETED the backup's copy, then `new local system snapshot 'fae76441...' did not match remote system snapshot 'fdf07779...'`, 5 failures, both SyncError. gw-b's files and its Canary tag are gone; gw-a still serves Canary.
    Recovery: gw-b 03:46:41.253 `[config-resource] Diff successfully applied. Synchronized snapshot manifest='fae76441...'`, both Good.
  implication: the filter hides the master's mode-collection tag resources from the diff but NOT the backup's (the backup's snapshot of a mode collection is unfiltered too), so the master computes "absent here, present there" and deletes them on the backup. A matching copy on the backup can never make the digests agree; writing both nodes is not a fix. Also: `ignition/tag-definition` itself is a resource of the collection (the type folder), which is why the empty folder in T6 counts.

- timestamp: 2026-11-02T03:47Z-03:49Z
  checked: |
    C5 03:47:23Z: a tag-type-definition unit (`production/ignition/tag-type-definition/default/DeployTypes/{udts.json,unary-resource.json}`, one UdtType) into gw-a, scan-lock + scan. Recovery: `rm -r production/ignition/tag-type-definition`, scan, resync.
    C6 03:48:26Z: a tag-group unit (`production/ignition/tag-group/default/DeployGroup/{config.json,resource.json}`, rate 2500) into gw-a, scan-lock + scan. Cleanup: `rm -r production/ignition/tag-group`, scan.
  found: |
    C5: wedged like T1 (master `Local snapshot='eea8b788...', diff=[]`, gw-b `'fae76441...' did not match`, 5 failures, both SyncError). Recovery succeeded (03:47:57 `Diff successfully applied ... 'fae76441...'`, both Good).
    C6: converged: gw-b `changeOps=[CreateResourceOperation{ignition/tag-group, production}, ...]` -> `Diff successfully applied` ('7c9d52bf...'); gw-b holds DeployGroup, config.json byte-equal; both Good. Its delete propagated too (03:48:58 back to 'fae76441...').
  implication: the affected set is exactly the filter's two types, `tag-definition` and `tag-type-definition`; `tag-group` (and by the same code `tag-provider`) units in a mode collection sync normally through config-resource.

- timestamp: 2026-11-02T04:39Z-04:50Z
  checked: |
    Option C implemented (gate `deploy_pair_tags_refused`, plan warning, rollback refusal, type-folder cleanup on every host). New live gates on the same 8.3.6 pair rig (still settled, trial still valid):
    L1 `live_pair_refuses_a_tag_unit` 04:39:09Z; L2 `live_pair_tag_cleanup_converges` 04:39:22Z (first version), 04:40:35Z (waits for gw-b's TEC-5105 "exceeds the max attempt count" line, the production state), 04:48:23Z (with the settle and no-resync assertions).
    Revert checks of L2 with `emptied_dir_floor` forced back to 3 (the empty `production/ignition/tag-definition/` left, as before the fix): 04:42:26Z and 04:44:39Z (the folder assertion turned into a print to see step 3).
  found: |
    L1 PASS: plan warns once naming ignition/tag-definition/default/Deploy; prepare refuses `deploy_pair_tags_refused` exit 6; gw-a's mode collection byte-identical, no tag-definition folder, no ign-deploy/ or .ign/, gw-b tap saw 0 requests, pair settled.
    L2 PASS (04:48): pre-fix canary + ledger + scan wedged the pair in 10 s (gw-b TEC-5105 line, both SyncError, no canary on gw-b). Recovery apply (canary `removed`, allow_destructive) deleted both files and the chain up to `ignition/tag-definition`; backup converged after the first 45 s budget + the apply's one resync (waited_s 50, files pass 4); pair settled at once. Following non-tag change (alarm settings liveEventLimit 7): applied, converged with waited_s 0 and resync None, gw-b holds 7, verify --node both ok, both nodes Good.
    Revert check (empty type folder left): the recovery apply ALSO reported converged (waited_s 50, resync sent, files pass) and step 3 too (waited_s 95, resync sent, gw-b got the change), but both nodes ended `SyncError`.
  implication: |
    (1) The fix works live: removing the folder chain recovers the pair, and afterwards it syncs a change on its own. (2) New, important: a resync pushes a diff's files across even while the digests still differ (consistent with T6d), so the `files` check passes on a still-wedged pair whenever the apply's one resync runs. `files` alone cannot tell a recovered pair from a wedged one; the discriminators are "converged without a resync" and "both nodes settle Good". L2 asserts both (and the folder's absence). projectState stays recorded, not a condition (V9); the follow-up recommendation is a warning when the backup converged only after the resync while projectState reads SyncError (not implemented here: out of the C scope).

## Eliminated

- hypothesis: the 2026-10-29 maintenance caused it. Eliminated: sync was healthy after the master restart and re-broke right after the 2026-11-01 apply.
- hypothesis: the canary content is part of the mismatched hash. Eliminated: the same hashes persisted with the canary present and absent until the master restarted.

- hypothesis: master in-memory snapshot staleness is what keeps the mismatch after rollback (rig). Eliminated on the rig by T1c/T1d: same gw-a process; mismatch with the empty `production/ignition/tag-definition/` folder present, match after only removing it. (The production "master restart clears it" observation is explained by T6b/T6c: a master restart turns projectState back to Good while config-resource stays stuck.)
  timestamp: 2026-11-02T03:36Z

- hypothesis: the canary's file shape (one-line JSON, `attributes.config` marker, folder unit) is what the backup cannot reproduce. Eliminated by C1: the identical bytes in `core/` converge through tag-config-resource.
  timestamp: 2026-11-02T03:38Z
- hypothesis: any file + scan write into a mode collection breaks the sync. Eliminated by C2 (live_pair_converges PASS, non-tag units in production converge).
  timestamp: 2026-11-02T03:39Z

- hypothesis: a master restart is a valid recovery (production observation "both nodes return to Good"). Eliminated on the rig by T6b/T6c/T6d: after the master restart both nodes read Good while gw-b's config-resource stayed stuck, a new non-tag unit never arrived, and a resync failed again with the empty folder still present.
  timestamp: 2026-11-02T03:44Z

- hypothesis: writing the same tag unit into the backup's mode collection too makes the digests agree (fix option "write both nodes"). Eliminated by C4: the master's diff deletes the backup's mode-collection tag resources, and the digests still disagree.
  timestamp: 2026-11-02T03:46Z

## Resolution

root_cause: "Ignition 8.3.6 config-resource sync cannot carry tag-definition or tag-type-definition resources that live in a MODE collection: ConfigResourceSynchronizer.getSnapshot counts every mode collection unfiltered while every diff (partial and full) is built with TAG_EXCLUDE_FILTER, and tag-config-resource syncs only `core`; ign deploy apply commits tag units into the mode collection (`production/ignition/tag-definition/<P>/<folder>/`), so the backup's digest can never match, the backup exhausts 5 full pulls and stops syncing config-resource; ign's rollback leaves the empty `production/ignition/tag-definition/` folder, which by itself keeps the pair wedged; a master restart only resets projectState to Good (false green) while config-resource stays stuck"

fix_options:
  A: "Route tag-definition and tag-type-definition units to the `core` collection (same staged file commit + scan-lock + scan). Proven by C1/C3. Costs: ign starts writing `core` (target.rs and mirror_mode_problem refuse it today); per-path collection routing through layout/plan/transport/ledger/rollback/pull/verify and the pair files check (gw-b reads them from core); tag units lose the mode overlay (CoreRelation::Hidden / ShadowsRemoval for tag folders), and ign owns core tag folders Designer may also edit; gw-b re-serializes unary-resource.json (needs a parsed-JSON row like K2, or ign writes the gateway's pretty form); migration of tag units already in a mode collection. Sub-choice A1 always, A2 only when the target has nodes.backup (independent gateways handle mode-collection tags, CD5)."
  B: "Write tag units through the gateway tag API (POST /data/api/v1/tags/import collisionPolicy Overwrite + tag deletes). Proven for create by C3 (core write, byte-equal on gw-b). Costs: a second apply mechanism beside files (no staged commit or single scan), explicit deletes and ordering because import never removes tags, rollback by export-before / import-after, repo tags.json/udts.json to import-payload conversion (folders, provider-root tags, _types_), more REST surface to capture and mock; same core-ownership question as A."
  C: "Refuse now: plan/preflight refuses an apply that writes or keeps tag-definition / tag-type-definition paths in a mode collection when the target has nodes.backup (new stable slug + README exit-table row), and the commit/rollback cleanup removes an emptied `<mode>/ignition/tag-definition/` and `tag-type-definition/`. Small and immediate; tag deploy to pairs stays unavailable until A or B."
  D (eliminated by C4): "write the unit into the backup's mode collection too: the master's diff deletes the backup copy and the digests still differ."
  always: "(1) remove the emptied type folder in a mode collection on delete/rollback (T6); (2) never accept projectState Good as convergence after a master restart: keep the files check authoritative (T6b/T6c); (3) production recovery runbook for the user (NOT run here): on the prod master remove every production/ignition/tag-definition and tag-type-definition path including empty folders, config scan, POST /data/api/v1/redundancy/gwaction/resync on the master, then ign deploy verify --node both and require files pass."

fix: |
  Option C (user decision "C now, then A"), a patch release after v1.6.4:
  (1) New stable slug `deploy_pair_tags_refused` (exit 6, four-place rule, README exit table, DEPLOY_SLUGS 17). `evaluate_gates` takes the target's `Redundancy` (Standalone | Pair, from `nodes.backup`); on a pair it refuses, after `plan_stale`/`deploy_unreadable` and before the no-op return, every `ignition/tag-definition` or `ignition/tag-type-definition` entry that is `added`/`changed` (written into the mode collection) or `same` with a mode copy (kept there) (`plan::pair_tag_keys`). `removed` (the recovery) and core-only `same` units pass. `run_apply` re-checks it (WR-04 pattern). `deploy plan` on a pair target adds one warning naming those units (not in plan_sha256). `deploy rollback` on a pair refuses a restore that would put a tag file (touched path with a pre-image) back into the mode collection; restores that only delete created tag files, and interrupted-run recovery, are allowed.
  (2) Delete and restore on every host remove an emptied `ignition/tag-definition` / `ignition/tag-type-definition` type folder (new `transport::emptied_dir_floor`, 2 for those two types, `MIRROR_MIN_COMPONENTS` 3 otherwise): local apply host, local mirror (unix and windows), `commit.sh` `rmdir_up`, `commit.ps1` `RmdirUp`, and both sftp script-report parsers accept exactly that floor.
  (3) Files check stays authoritative: `projectState` remains recorded, not a condition (V9); pinned by a unit test.
  (4) README: "Tags and redundant pairs" subsection with the production recovery runbook (T6e), apply order step 5, rollback order and paragraph, plan paragraph, exit codes, output contract.
verification:
  target_test: { result: pass, tests: [gates::tests::pair_tags_* (4), rollback::tests::pair_rollback_refuses_restoring_tag_files_into_the_mode_collection, error::tests::deploy_pair_tags_refused_four_places, layout::tests::pair_unsynced_tag_paths_are_the_two_filtered_types, sftp::tests::mirror_report_names_the_type_folder_only_for_the_pair_tag_types, pair::tests::project_state_good_never_outweighs_a_failed_files_check, deploy_apply.rs pair_* (4), deploy_host_conformance conformance_{local,sh}_{delete_emptying_a_pair_tag_type_removes_its_type_folder,restore_removes_a_created_pair_tag_type_folder} (ps rows wired for the Windows CI leg)] }
  mutation_check: { result: pass, method: "manual seeded mutants (no Stryker for Rust), scratchpad mutants.py", mutant_killed: "9 of 9: M1 gate off, M2 kept-same not counted, M3 engine guard off, M4 rust floor 3 (conformance), M4b rust floor 3 (engine test), M5 commit.sh floor 3, M6 rollback refusal off, M7 plan warning off, M8 sftp parser floor 3" }
  no_op_deletion: { result: pass, deletion_justified_by_rca: n/a, note: "additive: a new refusal and a deeper cleanup floor for two types; no branch removed, no assertion weakened" }
  adjacent_tests: { result: pass, suites_run: ["cargo fmt --all --check", "cargo clippy --workspace --all-targets -D warnings", "cargo test --workspace: 3098 passed, 0 failed, 94 ignored, 96 binaries", "PATH=$HOME/.cargo/bin:$PATH cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -D warnings", "commits a9635c4 and a977786 each tested alone (stash --keep-index): clippy + ignition-core lib + deploy_apply/deploy_plan/deploy_host_conformance + ignition-cli deploy_family_coverage/contract_deploy green"], goldens: "PLAN_GOLDEN, PLAN_GOLDEN_LEDGER, PLAN_GOLDEN_UNREADABLE, RENDER_GOLDEN, fingerprint GOLDEN unchanged", manifests: "no Cargo.toml or Cargo.lock change" }
  revert_and_reconfirm: { result: pass, bug_returned_on_revert: true, fixed_on_reapply: true, evidence: "live L2 with emptied_dir_floor forced to 3 (04:42Z, 04:44Z): the empty type folder stayed and both nodes ended SyncError; every change then needed a resync (waited 95 s). Reapplied (04:48Z, 04:50Z): folder gone, pair settled, the following change converged with no resync, both Good. The refusal's revert is the T1 reproduction itself (L2 step 1 re-wedges the pair every run)." }
  live: "all 6 pair gates PASS on the 8.3.6 pair rig 04:49:58Z-04:54:22Z (live_pair_converges, _with_a_stale_signature, _refuses_a_tag_unit, _secret_names, _tag_cleanup_converges, _unsynced_then_reconverge)"
  guardrail_verdict: accepted
files_changed:
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/src/deploy/layout.rs
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/deploy/apply/gates.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/apply/rollback.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/actions/deploy_apply.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs (test)
  - crates/ignition-core/src/deploy/transport/scripts/commit.sh
  - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
  - crates/ignition-core/src/deploy/pair/tests.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/deploy_plan.rs
  - crates/ignition-core/tests/deploy_host_conformance.rs
  - crates/ignition-core/tests/live_deploy_pair.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
  - README.md
next_step: "Option A (NOT implemented here): route tag-definition and tag-type-definition units to the `core` collection (same staged commit + scan-lock + scan), proven by C1/C3, so tags can be deployed to pairs again; then lift `deploy_pair_tags_refused`. See fix_options.A for the costs (per-path collection routing through layout/plan/transport/ledger/rollback/pull/verify, the pair files check reading core, unary-resource.json re-serialization on gw-b, core ownership, migrating units already in a mode collection). Also recommended (found by the L2 revert check): warn when the backup converged only after the apply's resync while projectState reads SyncError, because a resync pushes files across a still-wedged pair and `files` passes."

rig_state: "TORN DOWN 2026-11-02T05:30Z (`docker compose -p ign-pair-rig down -v`); the bind data dirs ~/.cache/ign-pair-rig/data-a|b remain and `setup-pair.sh up` recreates them fresh. The rig was settled and syncing when the last live run ended (04:54Z). Recreate with setup-pair.sh up/mint/pair/mode to re-run the live gates (about 7.5 min)."
