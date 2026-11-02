# Deferred items: quick 261102-cef

Out-of-scope findings logged during execution. Not fixed here.

## Pair rig driver deadlocks under bash 5.3 on macOS

- **Where:** `crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh`, `toggle` (and any other `jq ... <<<"$body"` on a large response body).
- **What:** with Homebrew GNU bash 5.3.15 as `bash` on PATH, `setup-pair.sh pair` hung in its relink loop. A process sample showed the subshell blocked in `heredoc_write` -> `write` on a pipe it would read itself: bash 5.3 writes a here-string into a pipe, and the body is larger than what the pipe takes, so the write never returns.
- **Workaround used:** `/bin/bash` (3.2) runs the script cleanly. The rig was linked with `/bin/bash ./setup-pair.sh link`, after a `docker compose -p ign-pair-rig restart gw-b`, and set to its mode with `/bin/bash ./setup-pair.sh mode`. The live gates ran with a PATH shim whose `bash` is `/bin/bash`, because the gates call `bash setup-pair.sh link|cut`.
- **Leftover:** the first `setup-pair.sh pair` process tree (started by this run) stays blocked in that write and could not be stopped from the agent session. Its root is the background `bash ./setup-pair.sh pair` started at about 2026-11-02T20:57Z. Stop it by hand (`pkill -f "setup-pair.sh pair"`).
- **Suggested fix:** write the body to a temp file, or pass it with `jq ... --argjson` / `printf '%s' "$body" | jq ...`, instead of a here-string.
