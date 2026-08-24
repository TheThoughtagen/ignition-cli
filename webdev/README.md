# webdev/

Gateway-side WebDev route sources for the ign CLI — the CLI's own tag,
alarm, history, and script-execution surface on the gateway (Phase 5).

Every tag operation in the CLI (`ign tags ...`) rides these routes; they are
deployed into a dedicated `ign-cli` project by `ign webdev deploy`, which
zips the bundle **embedded in `ignition-core`** (`src/webdev/mod.rs`) — no
source checkout is needed at deploy time.

## Layout (Designer-native)

```
routes/
  project.json                                # deploy project manifest
  VERSION                                     # route bundle version (handshake)
  com.inductiveautomation.webdev/
    resources/
      cli/
        tags/          (resource.json, config.json, doPost.py)
        tagConfig/     (resource.json, config.json, doPost.py)
        alarms/        (resource.json, config.json, doPost.py)
        tagHistory/    (resource.json, config.json, doPost.py)
        scriptExec/    (resource.json, config.json, doPost.py)
```

On the gateway the URL shape is `/system/webdev/ign-cli/cli/<route>`; each
route is a doPost action-dispatcher (`{"action": ...}`) answering the shared
`version` handshake with `{routeVersion, minCli}` matching
`ROUTE_BUNDLE_VERSION` / `MIN_CLI` in `crates/ignition-core/src/webdev/mod.rs`.

## Security posture

- `tags`, `tagConfig`, `alarms`, `tagHistory` are always-on and open at the
  WebDev layer (`require-auth: false` in `config.json`) — the CLI's own
  gateway credential still reaches the gateway; these routes carry no
  arbitrary-exec capability.
- `scriptExec` is **secret-gated in code** (planner-locked posture): its
  `doPost.py` is a TEMPLATE carrying the `__IGN_CLI_SECRET__` marker.
  `ign webdev deploy` generates the deployed copy by substituting the marker
  with a deploy-time hex secret, presented per call as the
  `x-ignition-cli-secret` (or `Authorization: Bearer`) header and compared
  constant-time in-route. Unconfigured or unsubstituted = every action
  (including `version`) is rejected — fail-closed. The template is
  deliberately NOT part of the embedded `ROUTE_FILES` bundle, so it cannot
  ship unsubstituted by accident.

## Conventions (do not "fix")

- Route folders are SELF-CONTAINED — no cross-resource imports; the shared
  core (unicode re-parse, `jv()` walker, body envelope) is duplicated across
  routes deliberately.
- Body envelope in every response: `{"ok": true, "data": ...}` /
  `{"ok": false, "error": {code, message, traceback?}}` — WebDev ignores the
  `'status'` key, so denials ride HTTP 200 and must be readable from the body.
- Every scripting call is the live-proven form from 05-RESEARCH.md; the
  research's nine prior-art defect corrections are pinned here (string-arg
  `getConfiguration`, basePath `configure`, alarms-as-list, 3-arg
  `acknowledge`, `t_stamp` column, kwargs-only `exportTags`, ...).
