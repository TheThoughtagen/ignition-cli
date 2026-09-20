---
schema_version: 1
open_count: 2
waived_count: 0
fixed_count: 0
total_count: 2
last_updated: 2026-09-20T06:10:18.133Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | quick/260919-tg4 | deviation | crates/ignition-cli/src/main.rs |  | guarded-marker parser rejected digits, silently dropping '// guarded:e2e init' | open |  | 2026-09-20T06:10:18.043Z |  |
| 2 | quick/260919-tg4 | deviation | crates/ignition-core/src/error.rs |  | network_error() test helper did not install the rustls provider; error::tests panicked standalone | open |  | 2026-09-20T06:10:18.133Z |  |

````json
[
  {
    "id": 1,
    "kind": "deviation",
    "phase": "quick/260919-tg4",
    "file": "crates/ignition-cli/src/main.rs",
    "line": null,
    "description": "guarded-marker parser rejected digits, silently dropping '// guarded:e2e init'",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-20T06:10:18.043Z",
    "resolved_at": null
  },
  {
    "id": 2,
    "kind": "deviation",
    "phase": "quick/260919-tg4",
    "file": "crates/ignition-core/src/error.rs",
    "line": null,
    "description": "network_error() test helper did not install the rustls provider; error::tests panicked standalone",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-20T06:10:18.133Z",
    "resolved_at": null
  }
]
````
