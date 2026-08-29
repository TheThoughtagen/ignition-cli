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
