---
status: resolved
trigger: "UAT 22 Test 2: tree from `ign deploy pull` fails `ign deploy verify` read_back with resource_differs on ignition/cobranding (appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType)"
created: 2026-10-15T20:30:00Z
updated: 2026-10-29
resolved: 2026-10-15
goal: find_root_cause_only
---

## Current Focus

bug_class: Bohrbug (deterministic)
status_note: ROOT CAUSE CONFIRMED (goal find_root_cause_only; no source edits made)
reasoning_checkpoint:
  hypothesis: "verify read_back applies N1 (drop JSON-null members) to the FILE config only and compares it against the RAW REST config; ignition/cobranding's REST view echoes the file's 9 null image members as explicit nulls, so after the one-sided strip they exist only in REST and diff_exact reports each as differing"
  confirming_evidence:
    - "pulled config.json nulls == the 9 finding paths exactly; captured REST singleton body has the same 9 keys as explicit null"
    - "real evaluate() on (pulled file, captured REST) reproduces the UAT finding byte for byte; stripping REST nulls or both sides -> Pass"
    - "verify.rs:610 diff_exact(&strip_nulls(file), rest, ..) — no strip on rest; REST config deserialised raw as serde_json::Value"
  falsification_test: "if evaluate() with both sides null-stripped still failed, or the raw file and raw REST were unequal, the hypothesis would be wrong — neither happened"
  fix_rationale: "make N1 symmetric (strip null members from the REST config too, at every depth) so null == absent on both sides; a non-null REST value against a file null still differs (repro E)"
  blind_spots: "UAT rig's own REST body for cobranding was not captured (rig torn down, scratch dir cleared); relied on the 22-09 capture of the identical 8.3.6 image + exact detail-string reproduction. Prod-gateway types with REST-side nulls beyond cobranding are unknown (only 46 rig items surveyed)."
  candidate_causes:
    - "code: one-sided strip_nulls in check_read_back (verify.rs:610/612) — CONFIRMED"
    - "environment/data: Ignition 8.3.6 serialises nulls per type (cobranding keeps explicit nulls, translator and every other surveyed type omits them) — contributing condition"
    - "process/test: CV2 cv2cmp compared RAW values, then N1 was generalised to all types without re-running cobranding; test mocks (rest_config helpers) hard-code 'REST strips nulls'; live verify only ran on the 2-resource tracer tree — why not caught"
  and_gate: "yes — the false red needs (1) the one-sided N1 compare AND (2) a declared type whose REST view emits explicit null members (cobranding with unset images, i.e. any default gateway). Pulled trees always include the cobranding singleton, so every pull of a default-branded gateway trips it."

## Symptoms## Symptoms

truth: A tree produced by `ign deploy pull` from a live gateway verifies green with `ign deploy verify` against that same gateway (read_back compares equal).
expected: UAT 22 Test 2 — pull then verify on the same gateway gives verdict green, exit 0.
actual: On local ign-deploy-rig (Ignition 8.3.6 b2026042713, trial, default cobranding), ign 1.5.0 @ 395f8ee: pull wrote gateways/rig-gw (93 files); verify --json exit 6, read_back fails resource_differs on ignition/cobranding, "differs at: appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType" (28 resources checked). Removing resources/ignition/cobranding -> green, exit 0, 27 resources.
errors: none beyond the finding above.
reproduction: UAT Test 2 in .planning/phases/22-transport-plan-pull-verify/22-UAT.md; scratch artifacts under scratchpad/uat22-t2/ (v1.json failing envelope, repo/ pulled tree, cobranding.bak removed resource dir).
started: Discovered during UAT 22; 22-09/22-10 live runs did not hit it.

## Eliminated

- hypothesis: pull/render bug — pulled cobranding tree content differs from what the gateway serves
  evidence: pull keeps bytes exactly as read and drops any unit that does not render byte-identically (D-09); raw pulled file == raw REST config as JSON values (repro line 1)
  timestamp: 2026-10-15T21:00:00Z

- hypothesis: binary image fields (logo/favicon/appIcon) stored as separate data files or base64 on one side, hash/default on the other
  evidence: resource.json files = ["config.json"] only; all 9 image fields are JSON null on BOTH the file and the REST view (8.3.6 default cobranding)
  timestamp: 2026-10-15T21:00:00Z

- hypothesis: gateway REST returns non-null defaults for the image fields (a genuine gateway quirk)
  evidence: 22-09 capture of the same 8.3.6 image with default cobranding shows explicit nulls; repro with that body reproduces the UAT detail string byte for byte
  timestamp: 2026-10-15T21:00:00Z

- hypothesis: same_scalar / child_path defect (numeric or path-building error)
  evidence: all differing paths are top-level string keys whose values are null; no number is involved; child_path output is correct
  timestamp: 2026-10-15T21:00:00Z

## Evidence

- timestamp: 2026-10-15T20:35:00Z
  checked: scratch uat22-t2/cobranding.bak/config.json (pulled file; read before the scratch dir was cleared on session restart)
  found: |
    {"appIcon":null,"appIconName":null,"appIconSize":null,"backgroundColor":"#697077","buttonColor":"#0C7BB3",
     "buttonTextColor":"#FFFFFF","enabled":false,"favicon":null,"faviconName":null,"faviconSize":null,
     "logo":null,"logoName":null,"logoType":null,"textColor":"#FFFFFF"}  (resource.json files: ["config.json"], scope A)
    The 9 null members are EXACTLY the 9 paths in the finding.
  implication: no binary/base64 image data involved; the differing keys are the file's JSON-null members.

- timestamp: 2026-10-15T20:36:00Z
  checked: scratch uat22-t2/v1.json (failing verify envelope)
  found: read_back fail, 28 checked, single finding resource_differs ignition/cobranding "differs at: appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType"; all other checks pass.
  implication: only cobranding trips; the other 27 resources (incl. ones with N1-relevant nulls) compare equal.

- timestamp: 2026-10-15T20:38:00Z
  checked: crates/ignition-core/src/deploy/verify.rs check_read_back / strip_nulls / diff_exact (lines 585-720)
  found: Compare::Exact runs diff_exact(&strip_nulls(file), rest, ...). strip_nulls is applied to the FILE side only; the REST `config` is used raw. diff_exact pushes every REST key absent from the (stripped) file object (`for key in r.keys().filter(|k| !f.contains_key(*k))`). Module doc: "N1 ... dropped from the file value first ... A REST `null` member where the file has none still differs."
  implication: if REST carries `"logo": null` etc., stripping them from the file only makes them REST-only keys -> reported as differing.

- timestamp: 2026-10-15T20:40:00Z
  checked: 22-LIVE-CAPTURES.md lines 1212-1260, GET resources/singleton/ignition/cobranding on the 22-09 capture rig (8.3.6, default cobranding)
  found: REST `config` = {enabled:false, backgroundColor, textColor, buttonColor, buttonTextColor, "logo":null, "favicon":null, "appIcon":null, "logoType":null, "logoName":null, "faviconName":null, "appIconName":null, "faviconSize":null, "appIconSize":null}
  implication: the gateway serialises the 9 unset image fields as explicit JSON null in the REST effective view — the SAME nulls the file has. Raw file == raw REST.

- timestamp: 2026-10-15T20:42:00Z
  checked: 22-LIVE-CAPTURES.md CV2 cv2cmp block (line ~2516) and CV2 normalisation table decision (lines 8166-8186)
  found: cv2cmp compared raw REST config vs raw file via `jq -S` -> "ignition/cobranding: equal (core)", row "equal | none". POSTGRES translator was the only N1 case (file has null createAutoIncSequence/createAutoIncTrigger, REST OMITS them). N1 was then generalised: "Applies to every type; compare after the drop, so a REST null member would still differ". Nobody re-ran cobranding through the N1-normalised compare; the decision table records cobranding's RAW result, not its post-N1 result.
  implication: N1 as specified (file-only strip, REST kept) is not a no-op on types where REST echoes the file's nulls. The CV2 table is internally inconsistent: cobranding is "equal | none" raw but differs under the universal N1 rule.

- timestamp: 2026-10-15T20:45:00Z
  checked: crates/ignition-core/tests/deploy_verify.rs:800 and crates/ignition-cli/tests/contract_deploy.rs:1364 (rest_config helpers feeding the mocked REST view)
  found: both mocks build the REST `config` as "the rendered config.json with JSON null members removed, as the gateway's REST view reports it (CV2 N1)".
  implication: the test suite encodes the false generalisation "REST always omits nulls"; no test ever feeds a REST body with an explicit null member, so the asymmetry is untested.

- timestamp: 2026-10-15T20:47:00Z
  checked: 22-10-SUMMARY live proof + crates/ignition-core/tests/live_deploy_plan.rs verify_green_then_red_live
  found: the live green verify ran on the 2-resource TRACER tree (POSTGRES translator + general-alarm-settings, resources_checked 2), not on a pulled tree. The pulled tree (95 files) was only fed to render+plan (pull_render_plan_is_empty_live). 22-14 produced no pull evidence.
  implication: pull -> verify round trip was never exercised live before UAT; explains why 22-09/22-10 did not hit it.

- timestamp: 2026-10-15T20:48:00Z
  checked: crates/ignition-core/src/deploy/pull.rs module doc (D-09 render fixpoint)
  found: "Bytes are kept exactly as read"; any unit whose rendered files are not byte-identical to its effective live files is dropped as not_round_trippable.
  implication: the pulled cobranding/config.json IS the gateway's file byte-for-byte. Pull/render is not at fault.

- timestamp: 2026-10-15T20:49:00Z
  checked: README.md 2935-2946 (deploy verify, read-back comparison CV2)
  found: "Members that are `null` in the file are dropped before comparing (N1) ... a tree made by `deploy pull` carries the full config and compares equal."
  implication: the README claim is falsified for any type whose REST view keeps null members (cobranding, at least).

- timestamp: 2026-10-15T20:52:00Z
  checked: every REST item body in 22-LIVE-CAPTURES.md (scratch script restnulls.py; 46 items parsed)
  found: ignition/cobranding is the ONLY REST item on the 8.3.6 rig whose `config` carries explicit null members (logo, favicon, appIcon, logoType, logoName, faviconName, appIconName, faviconSize, appIconSize). Every other type omits nulls (e.g. POSTGRES translator omits createAutoIncSequence/createAutoIncTrigger the file holds as null).
  implication: the gateway's REST serialisation of null is per-type (cobranding keeps, others drop). N1 handled only the "drop" direction.

- timestamp: 2026-10-15T20:55:00Z
  checked: crates/ignition-core/src/client/config_resources.rs ResourceRecord.config + deploy_api.rs resource_singleton
  found: `config: serde_json::Value` deserialised raw; nothing strips nulls on the REST path; serde_json::Value keeps null object members.
  implication: REST nulls reach diff_exact unchanged.

- timestamp: 2026-10-15T21:00:00Z
  checked: scratch repro crate (scratchpad/repro, path dep on ignition-core, CARGO_TARGET_DIR=~/Library/Caches/cargo-target-ign-cli) calling the real ignition_core::deploy::verify::evaluate
  found: |
    raw file == raw REST (serde_json Value eq): true
    A pulled file vs REST as captured (explicit nulls): read_back Fail "resource_differs ignition/cobranding differs at: appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType"  (byte-identical to UAT v1.json)
    B REST nulls removed: Pass
    C both sides stripped (symmetric N1): Pass
    D translator shape (file null / REST absent): Pass
    E file null / REST non-null: Fail at logo (real differences still caught)
  implication: CONFIRMED. The finding is produced purely by the one-sided strip; the two configs are equal as JSON. A symmetric null strip fixes it without masking a real difference.

## Resolution

root_cause: "Asymmetric N1 null normalisation in deploy verify read_back (crates/ignition-core/src/deploy/verify.rs check_read_back: diff_exact(&strip_nulls(file), rest) / diff_subset(&strip_nulls(file), rest)). JSON-null members are dropped from the file config only; the REST config is compared raw. Ignition 8.3.6 serialises ignition/cobranding's 9 unset image fields as explicit null in the REST effective view (same nulls as the on-disk file), so the file-side strip turns them into REST-only keys and diff_exact reports appIcon..logoType. The CV2 decision (22-LIVE-CAPTURES.md) recorded cobranding as raw-equal and then generalised N1 ('a REST null member would still differ') without re-checking cobranding under N1; test mocks encode 'REST omits nulls'; the live green verify ran only on the tracer tree."
fix: "Plan 22-15 made N1 symmetric: check_read_back null-strips the REST config as well as the file config before diff_exact and diff_subset, and diff_attributes strips both compared values. Commits: RED 90920e6, GREEN ba00845, guard tests 6a60a05, docs 4353824, live gate bc6bb24 (pull_then_verify_is_green_live)."
verification: "diagnosis only: scratch repro via real verify::evaluate — A (as captured) Fail with the exact UAT detail; B/C (REST stripped / both stripped) Pass; D translator N1 case Pass; E file-null vs REST-non-null still Fail"
files_changed: [crates/ignition-cli/tests/contract_deploy.rs, crates/ignition-core/src/deploy/verify.rs, crates/ignition-core/tests/deploy_verify.rs, crates/ignition-core/tests/live_deploy_plan.rs]
fix_verification: "22-UAT.md test 2 re-test result pass on a fresh 8.3.6 rig on 2026-10-15 (verify on the unmodified pulled tree exit 0, read_back pass, a Bad probe still exit 6), and 22-VERIFICATION.md status passed with G-22-2 in re_verification.gaps_closed."
closed_by: "quick 261029-fyc, 2026-10-29, from the existing record"
