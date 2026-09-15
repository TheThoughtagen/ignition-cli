# Phase 13 — LIVE GATE (13-04, the historian binding gate)

**Status: passed** — SC-3 + SC-4 closure evidence recorded on BOTH licensed trial rigs with the identical final gate binary (commit `65456a9`).

**Plan:** 13-04 · **Gate test:** `live_tags_history_bindings` (e2e_webdev.rs, `#[ignore]`, LIVE_GATE-serialized) · **Executed:** 2026-09-15 12:26–14:42 UTC · **Recipe:** 13-RIG-NOTES.md (commissioning wire steps, token provisioning, unique names, 2h windows)

---

## Final passing runs (the recorded evidence — both on `65456a9`)

| Rig | Image | Port | Window START (UTC) | Gate PASSED (UTC) | Duration | Verdict |
| --- | ----- | ---- | ------------------ | ----------------- | -------- | ------- |
| A (8.3.6) | `inductiveautomation/ignition:8.3.6` | 18188 | 14:37:09 (container create; fresh volume) | 14:42:03 | 89.5 s | **passed** — SC-3 ✅ SC-4 ✅ |
| B (8.3.3) | `inductiveautomation/ignition:8.3.3` | 19188 | 14:21:16 (fresh volume after the wedged-volume incident, §Incidents) | 14:29:25 | 54.6 s | **passed** — SC-3 ✅ SC-4 ✅ |

Both runs: fresh commissioning (09-02 wire recipe), fresh token (provision_token.sh, sanity `gateway-info` 200), gate self-deploys the current bundle first.

## Per-step verbatim outcomes — Rig A final run (14:40:34 → 14:42:03)

```
historian p13hist1789483235357 already gone (find → 404)          # pre-clean, corrected harness chain
deploy: mount race (a core route absent) — redeploying once       # the heal path fired live
historian p13hist1789483235357 provisioned (HTTP 200)             # native REST create
TAGS-13 live: history block present for [default]P13H/T… (provider p13hist…)   # SC-3 ✅
SC-4 closure evidence: written value 44 proven in history          # SC-4 ✅ (cycle 1, second poll)
historian p13hist1789483235357 deleted + verified gone (find → 404)            # teardown clean
test result: ok. 1 passed … finished in 89.46s
```

## Per-step verbatim outcomes — Rig B fresh run (14:28:30 → 14:29:25)

```
historian p13hist1789482886838 already gone (find → 404)
deploy: mount race (a core route absent) — redeploying once (11-06 lesson)     # heal fired, polled sweep passed
historian p13hist1789482886838 provisioned (HTTP 200)
TAGS-13 live: history block present for [default]P13H/T… (provider p13hist…)   # SC-3 ✅
history: no 44 yet (scan cadence 40–50 s) — rows so far: []        # first poll; second poll hit
SC-4 closure evidence: written value 44 proven in history          # SC-4 ✅
historian p13hist1789482886838 deleted + verified gone (find → 404)
test result: ok. 1 passed … finished in 54.61s
```

Rig B's TAGS-13 live render carried `history: / enabled: true / provider: p13hist… / sample mode: TagGroup` verbatim — the additive human block live-pinned on 8.3.3.

## Verdict lines

- **SC-3 (bindings visible): PASSED** — `ign tags config get` human output carried the `history:` block naming the LIVE provider on both rigs (8.3.6 + 8.3.3), wiremock-pinned for presence/absence/partial (contract_tags.rs `tags_config_get_history_render_contract`).
- **SC-4 (TAGS-14 done-state, closure branch): PASSED** — the 13-01 recipe (complete node + `[historyEnabled, historyProvider, sampleMode: "TagGroup"]`) re-proven end-to-end: bind → write 42/43/44 → the WRITTEN 44 returned as a history DATA row on both rigs. No null-data drift on the closure branch at the recorded runs.

## Drift vs the 13-01 spike record

**None at the recorded runs.** The recipe behaved exactly as captured: sampleMode=TagGroup binds the gateway default group (no group key sent, none needed), rows ride provider-relative with t_stamp preserved, and data flowed within the first ~1–2 polls. One environmental nuance learned (below) — it is a rig-hygiene truth, not a wire-truth change to the recipe.

## §Incidents (the hardening trail — every fix live-motivated, each committed separately)

1. **Deploy mount race (both rigs, repeatable):** the deploy import can half-land on a fresh gateway — rig A run 1 (12:26): three routes landed, `tags` answered Jetty-405 while tagConfig/tagHistory/alarms answered 1.3.0; a single redeploy-overwrite healed it (proven live both rigs). → Gate hardened to sweep-verify + one heal redeploy (`0ab99d6`), then the sweep itself was made a bounded 30 s poll after the heal's single-shot sweep raced a just-imported servlet's first activation on rig B (`65456a9`).
2. **Deploy-tail bind orphaning (rig A runs 2–4, 12:31–12:47):** a tag created inside the deploy import's model-rebuild tail reads back its config (TAGS-13 assert passed), answers writes Good, and NEVER registers with the historian — rows empty from birth. Identical probes created OUTSIDE that window (12:34, 12:47 manual) flowed the written 44 within seconds. The 11-06 same-name degradation class applies to tag paths too, so virgin names for BOTH historian and tag per run (`513ee87`), and the bind sequence runs in up to 3 self-healing cycles (settle → create → assert → write → bounded 75 s poll; the 11-06 delete→settle→recreate recovery, `76be84b`). Rig A's passing run exercised cycle 1; the recovery is live-proven.
3. **Wedged first boot on rig B (12:55–14:18, the expensive one):** the fresh 8.3.3 container's first boot STALLED in gateway initialization for ~35 min (JVM silent, wrapper pings degrading 8–27 s) while three unrelated containers hammered the host. A restart reached RUNNING (13:53) but the historian storage engine was dead-on-arrival for the REST of that volume's life: 4 gate cycles (15 s and 30 s settles) + a manual probe all produced config-present/tags-good/rows-empty, and the historian's namespace catalog registered ('default', 'System') while storing nothing. `down -v` + fresh volume → healthy boot in ~2 min → **the gate passed first-try, cycle 1** (54.6 s). Conclusion: the failures on the wedged volume were environment corruption, NOT 8.3.3 wire truth — consistent with 13-01's rig-B data proof on a healthy container. Never trust a volume that survived an interrupted first boot.
4. **Rig A trial expiry (~14:09):** the first rig-A window (12:09 → 14:09) lapsed mid-session; WebDev routes flip to 402 (the RouteStatus::Unlicensed marker) and historian storage stops. A was re-spun fresh for the final recorded run rather than re-arming a lapsed trial mid-mutation.

## Ops record

- Ports 18188/19188 LISTEN-free before each launch; container names `ign-p13-836` / compose project `ign-p13-833` (fresh `ign-p13-833_gateway_data` volume per generation).
- Commissioning: `commission.sh` (09-02 wire recipe) — bootstrap → eula 200/accept 201 → start-gateway 200 → poll StatusPing. NOTE: `StatusPing` answers `{"state":"RUNNING","details":"COMMISSIONING"}` while the commissioner still owns the port — poll `/data/api/v1/gateway-info` (401/200) for the real gateway-up signal.
- Tokens: `p13tok836` / `p13tok833` via provision_token.sh; NAME:KEY staged 0600 under /tmp/ign-p13-rigs/ (outside the repo; no secret material in-tree). A duplicate-name provisioning attempt fails silently in the script's jq gate — delete the orphan resource first (reprovision pattern used once on rig A).
- Teardown: rig A `docker rm -f`, rig B `compose down -v`. Post-teardown: **0 ign-p13 containers, 0 volumes** (verified twice, 14:36 and 14:42). Rigs torn down, NOT kept — the recipe above re-stages a rig in ~4 min.
- Manual probe tags/historians created during diagnosis were deleted before teardown (rig A: MProbe/MProbe2 + p13histprobe* deleted ~12:54; rig B probe resources lived on the destroyed wedged volume).

---

*Executed 2026-09-15, Task 3 of 13-04 (GSD executor). Gate commits in run order: `0ab99d6` → `513ee87` → `76be84b` → `65456a9` (final, the binary both recorded runs used). Verdict: passed.*
