# API Coverage — Phase 22 consumers of the Ignition 8.3.6 REST surface and system OpenSSH

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Phase 22 adds no new REST client method: it consumes Phase 20's `DeployApi` and the existing `GatewayApi` (logs, WebDev `tags` route) read-only, and it adds a system OpenSSH transport (`ssh`/`sftp` argv, no SSH crate). Every row below names what a Phase 22 verb or the transport uses, and why the rest is left out.

| capability | decision | reason |
|---|---|---|
| resources.list (effective view, verify read-back) | INTEGRATE | |
| resources.singleton (effective view, verify read-back) | INTEGRATE | |
| resources.names | OPT-OUT | not needed: plan and pull take live type and unit listings from the file transport (tags have no REST route), so the names route adds nothing |
| resources.find (effective view or ?collection=) | OPT-OUT | not needed: verify reads back one `list` call per type whose items carry `config`; per-name finds would multiply requests for the same answer |
| resources.create / resources.update | OPT-OUT | explicitly out of scope: every Phase 22 verb is read-only (DPLN-02, DPLN-06, DPLN-07); Phase 23 owns writes |
| resources.update allowInvalidReferences parameter | OPT-OUT | write-only parameter (present only on PUT/POST in `20-openapi-extract.json`), so reference validation cannot ride a read-back |
| scan.status (config) | INTEGRATE | |
| scan.trigger | OPT-OUT | explicitly out of scope: a standalone verify never triggers a scan; Phase 23 scans after apply |
| scan-lock.info / scan-lock.acquire | OPT-OUT | explicitly out of scope: plan, pull and verify take no lock (DPLN-02) |
| gateway-info.deploymentMode | INTEGRATE | |
| mode.list / mode.create | OPT-OUT | not needed: verify compares `deploymentMode` with the target; creating a mode is Phase 23 cutover work |
| logs.query (REST log entries since the last scan) | INTEGRATE | |
| WebDev `tags` route read (tag probes) | INTEGRATE | |
| sftp `ls -1` / `get -R` (read half, existence-first) | INTEGRATE | |
| sftp `mkdir` / `put` / `rename` / `rm` / `rmdir` (write half, no verb caller) | INTEGRATE | |
| ssh remote command (Windows PowerShell `-EncodedCommand`) | INTEGRATE | |
| sftp `copy` (copy-data extension, server-side copy) | OPT-OUT | not needed: commit moves staged files with `rename`; copy-data support on Win32-OpenSSH is unproven |
| ssh agent, X11 and port forwarding; ControlMaster multiplexing | OPT-OUT | explicitly out of scope: the hardened option set disables every side channel (`ForwardAgent=no`, `ForwardX11=no`, `ClearAllForwardings=yes`, `ControlMaster=no`) |
