# Phase 5 research probe route (live-verified 2026-08-24)

The corrected action-dispatch route deployed to a fresh 8.3.3 rig during
phase-5 research. This is a working skeleton for the Phase 5 route sources:
every action handler encodes a live-verified scripting-API correction
(string-arg getConfiguration, basePath configure, alarms-as-list, 3-arg
acknowledge, jv() manual serializer, Date(long) history windows,
exportTags kwargs + configure round-trip).

- `com.inductiveautomation.webdev/resources/cli/probe/doPost.py` — the route
- `.../config.json` — route gates (require-auth false for probing)
- `secure-config.example.json` — require-auth + role + user-source variant
  (verified: Basic works with it; API tokens 401)
- `../openapi-8.3.3-phase5-live.json` — trimmed 575-path live openapi capture
  (evidence for the resource-family decision)

Rebuild the deploy zip: zip root must contain project.json +
com.inductiveautomation.webdev/resources/... ; import via
POST /data/api/v1/projects/import/{name}[?overwrite=true] (application/zip).
