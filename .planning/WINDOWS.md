---
schema_version: 1
open_count: 1
waived_count: 0
fixed_count: 2
total_count: 3
last_updated: 2026-09-20T19:55:45.374Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | quick/260919-tg4 | deviation | crates/ignition-cli/src/main.rs |  | guarded-marker parser rejected digits, silently dropping '// guarded:e2e init' | fixed |  | 2026-09-20T06:10:18.043Z | 2026-09-20T11:47:25.145Z |
| 2 | quick/260919-tg4 | deviation | crates/ignition-core/src/error.rs |  | network_error() test helper did not install the rustls provider; error::tests panicked standalone | fixed |  | 2026-09-20T06:10:18.133Z | 2026-09-20T11:47:25.238Z |
| 3 | 15 | deviation | crates/ignition-core/src/error.rs |  | CoreError is exactly 128 bytes (ModuleDigestMismatch, 15-01, 5 Strings) — clippy::result_large_err fires workspace-wide (7 sites in ignition-tui: context.rs, workers/rig_stream.rs, lib.rs) under 'cargo clippy --workspace --all-targets -- -D warnings'. Pre-existing since 15-01, not introduced by 15-02 (ModuleDigestChanged is same-size, does not increase max variant size). Needs a workspace lint-policy decision (allow result_large_err, or box large variant payloads). | fixed | Boxed both payloads into ModuleDigestMismatchDetails/ModuleDigestChangedDetails (d0e92f7); Display moved onto the structs verbatim so rendered text, slugs and exit codes are unchanged. clippy --workspace -- -D warnings exit 0. | 2026-09-20T19:55:45.374Z | 2026-09-20T20:10:00.000Z |

````json
[
  {
    "id": 1,
    "kind": "deviation",
    "phase": "quick/260919-tg4",
    "file": "crates/ignition-cli/src/main.rs",
    "line": null,
    "description": "guarded-marker parser rejected digits, silently dropping '// guarded:e2e init'",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-20T06:10:18.043Z",
    "resolved_at": "2026-09-20T11:47:25.145Z"
  },
  {
    "id": 2,
    "kind": "deviation",
    "phase": "quick/260919-tg4",
    "file": "crates/ignition-core/src/error.rs",
    "line": null,
    "description": "network_error() test helper did not install the rustls provider; error::tests panicked standalone",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-20T06:10:18.133Z",
    "resolved_at": "2026-09-20T11:47:25.238Z"
  },
  {
    "id": 3,
    "kind": "deviation",
    "phase": "15",
    "file": "crates/ignition-core/src/error.rs",
    "line": null,
    "description": "CoreError is exactly 128 bytes (ModuleDigestMismatch, 15-01, 5 Strings) — clippy::result_large_err fires workspace-wide (7 sites in ignition-tui: context.rs, workers/rig_stream.rs, lib.rs) under 'cargo clippy --workspace --all-targets -- -D warnings'. Pre-existing since 15-01, not introduced by 15-02 (ModuleDigestChanged is same-size, does not increase max variant size). Needs a workspace lint-policy decision (allow result_large_err, or box large variant payloads).",
    "status": "fixed",
    "reason": "Boxed both payloads into ModuleDigestMismatchDetails/ModuleDigestChangedDetails (d0e92f7); Display moved onto the structs verbatim so rendered text, slugs and exit codes are unchanged. clippy --workspace --all-targets -- -D warnings exit 0, cargo test --workspace exit 0 / 67 suites.",
    "recorded_at": "2026-09-20T19:55:45.374Z",
    "resolved_at": "2026-09-20T20:10:00.000Z"
  }
]
````
