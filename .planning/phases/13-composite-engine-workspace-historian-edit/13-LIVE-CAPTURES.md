# Phase 13 — LIVE CAPTURES (13-01, the historian binding spike)

**Plan:** 13-01 · **Rigs:** trial 8.3.6 (`ign-p13-836`, :18188) + trial 8.3.3 (`ign-p13-833`, :19188) · **Replay executed:** 2026-09-15 02:07–02:15 UTC · **Zero product code** — this file is the spike record and the branch point 13-04 implements from.

---

## SPIKE VERDICT: closure — field set: [historyEnabled, historyProvider, sampleMode]

The tag↔historian binding was live-proven through the CLI tag write path on BOTH rigs: the three-key edit landed in tag config (read-back verbatim on both), and the historian returned DATA rows containing a written value (44) on both. 13-04 implements the TAGS-14 closure branch against the field-set table below; its own live gate re-proves the recipe end-to-end.

---

## §Aborted Designer branch (recorded per modified Task 3)

- **User decision 2026-09-15:** the Task-2 Designer step was DECLINED ("we can assume it works") — no Designer capture was performed on either rig. The Designer-diff oracle is therefore ABORTED, not failed; no `designer-diff-{836,833}.txt` artifacts exist and none are committed.
- **Replacement oracle (this file):** a live write-path replay through the CLI tag write path using the research hypothesis field set, with the read-back + data probe as the binding evidence. Recorded honestly per the modified-task verdict logic: binding landed AND read back on both rigs → closure recipe live-proven at config level, with data-level confirmation as a bonus beyond the required config-level proof.
- The Task-1 before-captures (`artifacts/config-before-{836,833}.json`, committed `0080dd2`) remain the pre-binding ground truth: both showed the unbound baseline (`dataType`/`defaultValue`/`name`/`path`/`tagType`/`value`, zero history keys), and both rigs' tags were re-verified byte-identical to those captures immediately before the replay.
- Rig windows were re-armed ~01:50Z (7199 s) for this session; teardown at 02:15:47Z — everything below fit inside one window per rig. Ops detail in 13-RIG-NOTES.md.

## §Write-path replay (the replacement oracle)

**Hypothesis field set** (research 13-RESEARCH.md checklist — confidently-named keys ONLY; the historical-tag-group key was deliberately NOT pre-committed since the aborted diff could no longer name it):

```json
{"historyEnabled": true, "historyProvider": "<per-rig provider>", "sampleMode": "TagGroup"}
```

**Per rig** (`ign tags config edit '[default]P13H/T1' --file -` on the existing tagConfig route — passthrough posture, no new CLI surface):

| Step | Rig A (8.3.6) | Rig B (8.3.3) |
| --- | --- | --- |
| edit request/response | `ok: true`, `quality: "Good"`, exit 0 | identical |
| read-back keys | `historyEnabled`, `historyProvider` (= `p13hist836`), `sampleMode` (= `TagGroup`) all present | identical (= `p13hist833`) |
| keys silently dropped? | **none** — every sent key landed | **none** |
| write refused? | no | no |

**Read-back side effect (wire truth for 13-04):** `tags config edit` REPLACES the whole node — the pre-edit `dataType: "Int4"` and `defaultValue: 0` were dropped, and `value` became `null`. The closure recipe must therefore send the COMPLETE node shape (`tagType`, `dataType`, value/defaultValue) plus the history keys in one edit, not the history keys alone.

Machine-diffed before→after (committed, `jq -S` normalized, never hand-edited): `artifacts/replay-diff-836.txt`, `artifacts/replay-diff-833.txt`. The after shapes themselves: `artifacts/config-after-{836,833}.json` (these are the REPLAY read-backs, not Designer captures).

## §Data probe (supplementary evidence, both rigs)

Three `ign tags write` values (42/43/44, ~1 s apart) then `ign tags history query` (±5 m window):

| Rig | Query 1 rows | Re-query rows | Written value in history? |
| --- | --- | --- | --- |
| A (8.3.6) | `(02:11:34.303, 0)`, `(02:12:14.298, 44)` — 2 rows | — | **YES** (44) |
| B (8.3.3) | `(02:11:29.001, 0)` — 1 row | `(02:11:29.001, 0)`, `(02:12:19.917, 44)` — 2 rows | **YES** (44, at next group scan) |

The 40–50 s row cadence is the tag-group scan storing at intervals — consistent with `sampleMode: TagGroup` binding to a default historical group (see next section). Rig B's first query simply preceded its next scan; the re-query captured the 44. Both rigs symmetric: **DATA rows, real stored values.**

## §Research Open Questions — answered with rig evidence

**OQ1 (the `historicalProvider` typo): CONFIRMED.** `historyProvider` (the doc-corrected name) landed, read back, and bound with live data flow on both rigs. 05-06's `historicalProvider` is retired as the wrong key — answering the plan's "typo?" question with evidence, not inference.

**OQ4 (provider-level binding state): binding lives IN the tag config.** The read-back proves the per-tag binding state is stored on the tag node itself — no provider-side per-tag state was needed. The provider's own record (via the corrected find route below) carries only storage settings (`timeLimit` 1 WEEK / `pointLimit` 10 M / `remoteSync` off) — nothing per-tag. The Designer's out-of-band-state concern from research PITFALLS does not materialize for InternalHistorian.

**Bonus finding — the historical-tag-group key is NOT required for a functional binding.** With `sampleMode: TagGroup` and NO group key sent, both gateways bound to a default historical group (evidenced by the periodic scan cadence storing rows). The `historicalScanclass` mystery key from research therefore never enters 13-04's recipe — it is absent from every capture here and stays uncommitted to any pinned artifact, exactly as the plan's no-guessed-shapes rule demanded.

## §Provider API wire finding (corrects the recorded delete shape)

The plan's recorded find route — `GET /data/api/v1/resources/com.inductiveautomation.historian/historian-provider/find/{name}` (the e2e_webdev.rs:899 shape) — **does not exist**: it returns Jetty HTML 404 "No route match" on both rigs. So do the collection GET and `GET {create-path}/{name}` — on the historian-provider mount only the POST (create) route exists.

The WORKING resources-API routes (live-proven on both rigs, consistent with the tag-provider/eam precedents in tags_contract.rs / eam_contract.rs):

| Verb | Route |
| --- | --- |
| find (one, incl. `signature`) | `GET /data/api/v1/resources/find/{module}/{type}/{name}` |
| list (collection) | `GET /data/api/v1/resources/list/{module}/{type}` |
| create | `POST /data/api/v1/resources/{module}/{type}` (ARRAY body) |
| delete | `DELETE /data/api/v1/resources/{module}/{type}/{name}/{signature}` |

⚠️ **Follow-up for 13-04 (or review-backlog):** the e2e harness's `delete_internal_historian` (e2e_webdev.rs:899) uses the malformed find URL and treats any non-200 as "already gone" — meaning its historian DELETE has always been a silent no-op. This plan fixes nothing in product/test code (zero-code plan); 13-04 should correct the harness find/delete URLs when it lands TAGS-14.

## §Field-set table (the 13-04 implementation contract — captured names ONLY)

| Key | Type | Captured value | Designer UI label (per docs table, pending any future Designer capture) |
| --- | --- | --- | --- |
| `historyEnabled` | Boolean | `true` | History Enabled |
| `historyProvider` | String | `p13hist836` / `p13hist833` | Storage Provider |
| `sampleMode` | Enum | `"TagGroup"` | Sample Mode |
| — (not required) | — | historical tag group: gateway default used | Historical Tag Group |

No key name appears above that is absent from the committed after-captures. `historicalDeadbandMode`/`historicalDeadband` (known-name, non-required keys) were not sent and not needed for binding.

## §Cleanup record

Per rig, scripted (`/tmp/ign-p13-rigs/cleanup.sh`): spike tag `tags config delete` (deleted: 1 both; final get reports `tag_type: "Unknown"`) → provider find (signature extracted) → `DELETE …/{name}/{signature}` (success: true, HTTP 200 both) → verified gone (find → 404, list → 0 items). Rigs torn down 02:15:47–02:15:50Z; no `ign-p13*` containers or volumes remain. Window END recorded in 13-RIG-NOTES.md.

---

*Executed 2026-09-15, modified Task 3 of 13-01 (GSD executor, continuation run). Verdict: closure.*
