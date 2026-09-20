---
phase: 14-transports-mcp-lsp
plan: 05
subsystem: transports
tags: [lsp, neovim, nvim-client, dual-client, composition, ignition-nvim, ide-03, headless-verification]

# Dependency graph
requires:
  - phase: 14-transports-mcp-lsp (14-04)
    provides: the contract-pinned `ign lsp` server surface (narrow completion/hover/diagnostics caps, TTL-stamped payloads) that the nvim client patch attaches to
  - phase: 14-transports-mcp-lsp (14-03)
    provides: the LSP transport loop + GatewayCache whose binary the headless e2e drives
provides:
  - "ignition-nvim registers a SECOND LSP client 'ignition_live' (cmd {'ign','lsp'}) beside the Python statics client, guarded by vim.fn.executable('ign')"
  - "Headless e2e proof: both clients attach on one ignition buffer — ignition_live advertises completion+hover with definition/codeAction/workspaceSymbol all FALSE (narrow-cap composition verified from the client side); statics keeps all five capabilities"
  - "Guard negative proof: ign absent → config never registered, no spawn loop"
  - "Patch committed in the ignition-nvim repo (0d6bd55) on branch claude/ign-lsp-live-client following its per-feature-branch + conventional-commit conventions"
affects: [14-06, IDE-03 acceptance, SC-3/SC-4 milestone closure]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Additive dual-client registration: a second vim.lsp.config + its own guarded FileType autocmd placed BEFORE the statics early-return, so the statics path stays byte-identical AND the live client works on venv-less machines"
    - "Guard-by-absence: executable() check gates registration (not just start), mirroring the existing venv/exepath checks"
    - "Headless attach verification with capability assertions per client — the composition property is machine-checkable, not observational"

key-files:
  created: []
  modified:
    - /Users/pmannion/whiskeyhouse/ignition-nvim/lua/ignition/lsp.lua (via symlink → packages/ignition-nvim/lua/ignition/lsp.lua)

key-decisions:
  - "Separate guarded FileType autocmd for ignition_live instead of adding a start inside the existing callback — the existing callback early-returns when ignition_lsp is already attached (skipping a co-located live start in the NORMAL case) and doesn't exist at all when the venv is absent; a sibling autocmd keeps the statics path byte-identical and serves both machine shapes"
  - "ignition_live registered BEFORE M.setup's statics early-return so it registers even when no Python ignition-lsp is found (Task 2's LIVE_ONLY fallback depends on this)"
  - "Commit landed on a NEW branch claude/ign-lsp-live-client off the checkout's current HEAD (fix/preview-flex-spill) — the sibling repo was mid-WIP on an unrelated fix branch with a dirty SKILL.md; per-feature-branch + conventional commits IS that repo's convention, and their WIP branch/dirty file were left untouched"
  - "Filetypes and root_markers mirror the live statics config exactly (ignition/python/ignition_expr, project.json); no settings/init_options — ign lsp resolves its profile ambiently per research OQ 4"

# Metrics
duration: 21 min
completed: 2026-09-16
---

# Phase 14 Plan 05: ignition-nvim Live-Client Wiring Summary

**ignition-nvim now registers `ign lsp` as a second (live-truth) LSP client beside the Python statics server — headless e2e proves BOTH clients attach on one ignition buffer with ignition_live advertising completion+hover and zero capability overlap, committed as 0d6bd55 in the sibling repo**

## Performance

- **Duration:** 21 min
- **Started:** 2026-09-16T11:19:20Z
- **Completed:** 2026-09-16T11:40:43Z
- **Tasks:** 2
- **Files modified:** 1 (sibling repo lsp.lua, 48 insertions / 0 deletions)

## Accomplishments
- The additive patch: `vim.lsp.config('ignition_live', { cmd = {'ign','lsp'}, root_markers = {'project.json'}, filetypes = {'ignition','python','ignition_expr'} })` + its own FileType autocmd (with the plugin's virtual-buffer root resolution mirrored), all inside a `vim.fn.executable('ign') == 1` guard
- Headless end-to-end verification: `ATTACHED_COUNT=2`, `CASE=BOTH_CLIENTS`, ignition_live caps `{completion=true, hover=true, definition=false, codeAction=false, workspaceSymbol=false}` vs statics all-true — nvim's native merge composes the two servers, neither knowing about the other (SC-3 mechanical proof)
- Guard negative: with ign stripped from PATH, `IGNITION_LIVE_CONFIG_REGISTERED=false` — no failed-spawn loop (Pitfall 5 closed)
- Re-ran the e2e against the COMMITTED state (0 diff lines working-tree-vs-HEAD) — the verified artifact is the committed artifact

## Task Commits

The plan places the sibling-repo commit in Task 2 (patch → verify → commit → re-verify committed state), so the single-file patch carries ONE atomic commit serving both tasks:

1. **Task 1 + Task 2: lsp.lua patch + headless verification** — `0d6bd55` in ignition-nvim (`feat(lsp): register ign lsp as a second (live-truth) client beside ignition-lsp`, branch `claude/ign-lsp-live-client`)

**Plan metadata:** (this repo) `docs(14-05): complete ignition-nvim live-client wiring plan`

## Headless Verification Evidence (verbatim)

```
IGN_EXEPATH=/Users/pmannion/Library/Caches/cargo-target/debug/ign
IGN_EXECUTABLE=true
FILETYPE=ignition
BUFFER=/private/tmp/ign_lsp_e2e_proj/ignition/resource.json
STATICS_EXPECTED=true
ATTACHED_COUNT=2
CLIENT name=ignition_live id=1 root=nil completion=true hover=true definition=false codeAction=false workspaceSymbol=false
CLIENT name=ignition_lsp id=2 root=nil completion=true hover=true definition=true codeAction=true workspaceSymbol=true
CASE=BOTH_CLIENTS statics+live coexist (nvim native merge)
E2E_PASS ignition_live attached with completionProvider+hoverProvider
EXIT=0
```

(The verified binary was this phase's fresh build: `/Users/pmannion/Library/Caches/cargo-target/debug/ign`, mtime 2026-09-16, built from ignition-cli HEAD `b3d6245`, PATH-scoped into the nvim run — NOT the stale v1.0.0 `~/.cargo/bin/ign`.)

## Files Created/Modified
- `/Users/pmannion/whiskeyhouse/ignition-nvim/lua/ignition/lsp.lua` (→ `packages/ignition-nvim/lua/ignition/lsp.lua`) — second client registration + guarded dual-start autocmd; statics chain byte-identical (48+/0-)

## Decisions Made
- Sibling guarded autocmd over co-located start (rationale above — forced by the additive-only must-have)
- Registration placed before the statics early-return (LIVE_ONLY fallback requires it)
- New `claude/ign-lsp-live-client` branch in the sibling repo (their WIP branch + dirty file untouched; PR-merge flow preserved)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Build output location — repo `target/debug/ign` was a stale Sep-10 leftover**
- **Found during:** Task 2 (first e2e run — `ign lsp` answered "unrecognized subcommand 'lsp'", client quit exit 2)
- **Issue:** the user's global `~/.cargo/config.toml` sets `target-dir = /Users/pmannion/Library/Caches/cargo-target`, so `cargo build` output never lands in the repo's `target/`; the on-disk `target/debug/ign` (Sep 10) predated 14-03/14-04 and lacked the `lsp` subcommand entirely
- **Fix:** PATH scope changed to `/Users/pmannion/Library/Caches/cargo-target/debug` — the genuinely fresh binary (verified: `ign lsp --help` works, mtime today, built from HEAD b3d6245)
- **Files modified:** none (environment discovery; scratch harness PATH only)
- **Verification:** e2e IGN_EXEPATH line shows the cache-dir binary; `ign lsp --help` renders the hidden subcommand
- **Committed in:** n/a (no repo change)

**2. [Rule 1 - Bug] Scratch e2e harness raced the slower Python statics server**
- **Found during:** Task 2 (second e2e run reported CASE=LIVE_ONLY on a machine where the venv IS present)
- **Issue:** the wait loop polled only for ignition_live; the native binary attaches in ~200ms and the script exited `qa!` before the Python server finished initialize — run 1 only looked correct because the broken `ign` made the loop burn its full 15s window while Python attached
- **Fix:** harness now waits for ignition_live caps AND the statics client (when venv-expected), and FAILS if statics is expected but never attaches — LIVE_ONLY can no longer be misreported on a venv-present machine
- **Files modified:** /tmp scratch only (never committed, per plan)
- **Verification:** re-run → ATTACHED_COUNT=2, CASE=BOTH_CLIENTS
- **Committed in:** n/a (scratch)

---

**Total deviations:** 2 auto-fixed (1 blocking env discovery, 1 harness bug). **Impact on plan:** both were required to make the e2e evidence trustworthy (right binary; right wait condition). Zero product-code scope creep — the lsp.lua patch is exactly the plan's additive shape.

## Issues Encountered
- Research line anchors had drifted from the live file (M.setup now 7-95, find_lsp_server 98-127, guard checks 103-118) — re-derived from the actual code per the plan's re-verify directive; the patch shape held.
- First e2e failure was diagnosed, not retried: lsp.log + a manual `ign lsp` run pinpointed the stale binary (clap "unrecognized subcommand"), root-caused to the global cargo target-dir redirect.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- SC-4 (mechanical half) is DONE: `ign lsp` is in ignition-nvim's detection order, verified end-to-end headless with BOTH clients
- SC-3 (composition) holds at the client boundary: statics ownership intact (definition/codeAction/workspace symbols), live client narrow (completion/hover/diagnostics), nvim native merge composes
- Remaining for the phase: 14-06 only; the sibling patch awaits the user's own merge of `claude/ign-lsp-live-client` (branch off their WIP `fix/preview-flex-spill` HEAD — merge order is their call)
- No blockers carried forward

---
*Phase: 14-transports-mcp-lsp*
*Completed: 2026-09-16*

## Self-Check: PASSED

- Files: 14-05-SUMMARY.md exists; sibling `packages/ignition-nvim/lua/ignition/lsp.lua` patched (4 `ignition_live` occurrences, 48+/0- diff)
- Commit: `0d6bd55` present in ignition-nvim git log (branch `claude/ign-lsp-live-client`)
- must_have greps: `ignition_live` in lsp.lua ✓; `cmd = { 'ign', 'lsp' }` ✓; `vim.fn.executable('ign')` guard ✓; capability-claim words (`definition|code_action|workspace_symbol`) absent from lsp.lua ✓; statics chain byte-identical (additive-only diff) ✓
- Verification at close: headless e2e BOTH_CLIENTS (attach caps asserted per client), guard-negative run (no registration without ign), committed-state re-run (0 diff vs HEAD), fresh-binary evidence (IGN_EXEPATH = cargo-target cache build of b3d6245)
