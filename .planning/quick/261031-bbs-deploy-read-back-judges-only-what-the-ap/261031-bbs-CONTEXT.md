# Quick Task 261031-bbs: deploy read_back judges only what the apply changed - Context

**Gathered:** 2026-10-31
**Status:** Ready for planning

<domain>
## Task Boundary

Make `ign deploy apply` usable on the production pair (WIGNVM01/WIGNVM04, Ignition 8.3.6). This ships as v1.6.4.

**Live evidence (2026-10-30, run `20261030T213754Z-28f01c`).** The first production apply (one added memory-tag unit, `ignition/tag-definition/default/Deploy`, plan `3521fc17…`) went like this:
- It wrote 2 files and one scan loaded them: `files` and `loaded` passed.
- The post-apply verify failed `read_back`, and apply rolled back (`deploy_rolled_back`). The rollback was clean.
- A standalone read-only `ign deploy verify --env production` afterwards showed that `read_back` fails on **23 resources the apply never touched**. They are 22 `resource_differs` and 1 `resource_missing`:
  - **8 historian providers:** `com.inductiveautomation.historian/historian-provider/*` (CoLOS, SparkDB, WHK01_PG_*, cmms, mes), differing at `settings.profile`.
  - **5 OPC UA resources:** `com.inductiveautomation.opcua/device/*` (Chiller1/2, Viewmarq_Barreling at `settings.deviceSettings`; Markem Printer at `settings.connectivity.writeTimeout` and `settings.write.timeout`) and `opcua/server-config` (`advanced.gdsPushEnabled`, `endpoint.messageSecurityModes`).
  - **7 Cirrus Link resources:** `mqtt.engine.gateway/general` (`blockCommands`, `useCirrusEncoder`), `namespace-server-set/1,2,7,8` (at the root), `mqtt.transmission.gateway/history-store/Default In-Memory Store` (4 fields), `transmitter/UNS SpB` (`useCirrusEncoder`).
  - **3 core resources:** `ignition/system-properties` (`enableTagReferenceStore`, `homepageNotes`), `ignition/gateway-network-settings` (`dataChannelQueueSize`, `dataChannelThreadPoolMaxSize`, `whitelist`), and `ignition/edge-sync-settings`, which is not in the gateway's effective view.

The plan reports all 23 as `same`: the files on disk equal the declared tree byte for byte. The gateway's REST view of them differs from the files, through filled defaults, normalisation, or module-specific shapes. `read_back` judges every declared resource, so **every production apply rolls back**. Stock rigs never showed this because their resources are stock.

**Out of scope:**
- Understanding or modelling each module's REST normalisation.
- Changing what `files`, `loaded`, `scan`, `mode` or `probes` judge.
- A Cargo.toml bump (release.yml stamps the version from the tag).
- Any write to production. The re-run of the canary apply happens after release, separately.

</domain>

<decisions>
## Implementation Decisions

### Post-apply read_back (user: "Baseline diff")
- Before the write (inside apply, before the lock or at least before commit), apply records a **REST baseline**: the REST view of every declared resource that `read_back` would compare.
- After the scan, read_back **fails** only on:
  - (a) a unit this run wrote, whose post-apply REST view does not match what was declared. Researcher/planner: define "match" so that the gateway's own normalisation of a written resource does not false-fail. Recommended approach: compare the set of differing JSON paths. A path that already differed between REST and file in the baseline, for the same resource, is not new. A brand-new unit with no baseline is judged as today, and the planner documents the risk.
  - (b) a resource this run did NOT write, whose REST view changed from the baseline. That means the apply disturbed something it should not have.
- A difference that already existed in the baseline (REST vs file, unwritten resource, same as before) is a **warning**, never red. Each warning names the resource and the differing paths, as today's findings do.
- A deleted unit: the REST view must show it absent (as today).
- Rollback stays automatic on a real read_back failure. No flag skips verify; the README contract "no flag skips the plan hash, the verify or the rollback" stays true.

### Standalone `deploy verify` (user: "Warn, not red")
- Without a baseline, read_back differences become **warnings** (`data.warnings`, and in the check's findings with `status: warn`). Verify is green when scan, mode, files and loaded pass.
- `files` still catches real on-disk drift.
- The `read_back` check status for verify becomes `warn` when there are only differences, and `fail` only for something real. The planner decides whether `resource_missing` (declared but absent from the effective REST view, such as `edge-sync-settings`) stays red. Recommended: warn when the plan says the files are `same`, because the files exist and are loaded.

### Contract
- Additive only: new warning codes or fields, with no rename or removal. The four-place rule applies if a slug is added; prefer none.
- README: the verify checks table (read_back row), the apply section (baseline), and the "verdict exception" text if it changes.
- Goldens: `PLAN_GOLDEN*` and `RENDER_GOLDEN` are untouched (plan is unchanged). Any verify or apply envelope golden that changes is updated deliberately and explained.

### Claude's Discretion
- Where the baseline lives in the run directory (for example `run_dir/read_back_baseline.json`, with no secrets: REST views can carry secret-shaped fields, so store only the differing JSON paths and hashes, never values).
- The names of the new warning codes, for example `read_back_preexisting`.
- Whether the baseline read happens before `lock_acquired` (outside the hold budget). Recommended: before, because a 23+ resource REST sweep should not eat the 176 s lock hold.

</decisions>

<specifics>
## Specific Ideas

- **Reproduce offline:** a wiremock or fake gateway where the REST view of an unwritten resource differs from its file (an extra default field). The apply must succeed, and that difference must appear as a warning. A second case changes an unwritten resource's REST view between baseline and post-scan: the apply must fail and roll back. A third case writes a unit whose REST view adds a default field that was already differing in the baseline: it must pass.
- **Run-record evidence:** the run's `verify_checks` event currently carries only statuses. Add the finding subjects (paths only, no values), so a rollback can be diagnosed from the run directory without re-running verify. This would have saved 15 minutes on 2026-10-30.
- **Live check (no production):** on the Docker 8.3.6 rig, make a resource whose REST view differs from its file (find one the gateway normalises, or reuse a module default), then show apply succeeding with a warning. Optionally repeat on the Windows VM. The planner decides; Docker alone is acceptable here.

</specifics>

<canonical_refs>
## Canonical References

- README: the `deploy verify` checks table (`scan`, `mode`, `read_back`, `files`, `loaded`, `probes`), `deploy apply`, and the verify verdict exception (2026-10-14).
- `crates/ignition-core/src/deploy/verify.rs` (read_back), `src/deploy/apply/` (the post-apply verify, the rollback and events), `src/actions/deploy_apply.rs`.
- `.planning/milestones/v1.6-phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md` (apply and verify on rigs).
- The live production evidence: the run directory `~/whiskeyhouse/whk-onprem-infra-iac-ign-canary/ignition/.ign/deploy/wignvm01/20261030T213754Z-28f01c/` (events.ndjson, result.json) and the verify output summarised above. Do not copy the `.gwbk` or the snapshot anywhere.
- `.claude/CLAUDE.md`: no worktrees, the shared CARGO_TARGET_DIR, and install before smoke-checking.

</canonical_refs>
