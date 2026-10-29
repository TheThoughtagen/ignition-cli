---
status: resolved
trigger: "07-UAT round 1 test 6: ign eam task new uat-backup-demo eam_backup got HTTP 422 'Settings cannot be null' from the 8.3.3 gateway, surfaced as internal_error"
created: 2026-08-29
updated: 2026-10-29
resolved: 2026-08-29
goal: find_root_cause_only
evidence_files: [eam-working-definition.json, eam-history-raw.json]
---

# EAM create 422 evidence (2026-08-29, gateway A :9088, 8.3.3)

CLI composed body (actions/eam.rs eam_task_new):
[{"name":"uat-backup-demo","config":{"profile":{"type":"eam_backup","scheduleMode":"OnDemand","targetGateways":[]}}}]

Gateway response: HTTP 422
{"messages":["Settings cannot be null"],"fieldMessages":[]}

Working definition (cli-research-backup, find read-back — eam-working-definition.json):
config.profile: {type, isSuspended: false, scheduleMode: "OnDemand"}
config.settings: {targetGateways: ["_controller"], targetGroups: [], concurrentBackups: 0, forceBackups: false}
→ targetGateways belongs in config.SETTINGS; config.settings is REQUIRED (null → 422).

Also: the 422 falls through classify() to internal_error ("internal errors are bugs")
— a 4xx validation response surfacing as internal error is itself a taxonomy gap.

## Resolution

root_cause: "The create body put targetGateways in config.profile and sent no config.settings, which the gateway requires (null gives 422). Secondary: classify had no arm for a config-resource 4xx, so the 422 surfaced as internal_error."
fix: "Plan 07-05 Task 2, commit 4bdbb81. compose_task_definition splits profile from settings, and zero targets default to _controller. A path-scoped 422 arm (is_config_resource_url) maps the gateway messages to invalid_input, exit 2."
verification: "Confirmed from the existing record on 2026-10-29: 4bdbb81 and 6e8236f are ancestors of HEAD; crates/ignition-core/src/actions/eam.rs still defines compose_task_definition and crates/ignition-core/src/client/classify.rs still defines is_config_resource_url; 07-VERIFICATION-GAPS.md status passed with truth 3 VERIFIED and gap row 3 (eam task new 422) CLOSED; 07-05-SUMMARY.md records the live create of uat-backup-demo exiting 0 on gateway A (:9088); 07-UAT.md round 2 Summary shows passed 5, issues 0."
companion_evidence: "eam-working-definition.json is the working definition read back, the basis for the fix. eam-history-raw.json is the raw eam history capture behind the 07-UAT round 1 UUID taskId gap, closed by 07-05 Task 1 commit 6e8236f."
files_changed: [README.md, crates/ignition-cli/tests/contract_eam.rs, crates/ignition-core/src/actions/eam.rs, crates/ignition-core/src/client/classify.rs, crates/ignition-core/tests/eam_contract.rs]
caveat: "In the archived 07-UAT.md round 2, the test 2 line still reads result: [pending] while the round's Summary counts 5 of 5 passed. The archived file was not edited."
closed_by: "quick 261029-fyc on 2026-10-29"
