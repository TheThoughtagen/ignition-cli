---
name: ign-workspace
description: Local editing workflows with ign — workspace checkout/status/push over the manifest-verified diff engine, and the ign edit round-trip for single resources. Use when editing Ignition project resources outside the Designer.
user-invocable: false
---

# ign Workspace & Edit

Edit Ignition project resources in a real editor / agent workflow, then push back guarded. macOS + Linux only. **Tag values never appear in workspace trees** — values are runtime state; tags stay in `ign tags` verbs.

## The workspace loop (project-scale)

```bash
ign workspace checkout MyProject --json          # read-only on the wire: one export GET, zero imports
# ... edit files locally (nvim + ign lsp recommended; see below) ...
ign workspace status --json                      # drift report — states are PUSH-RELATIVE
ign workspace push --yes --json                  # guarded; refusal message IS the blast radius
```

### Semantics worth knowing

- **The manifest is the identity.** `.ign-workspace.json` records the gateway↔local path mapping at checkout — `status`/`push` read it, never re-derive. Commit it; the generated `.gitignore` excludes codec artifacts.
- **Status rows are push-relative**: `local_edit` = push would write, `gateway_drift` = untouched locally, `conflict` = **push refuses even with `--yes`** (both sides moved — the clobber class; reconcile by hand).
- **Deletions are opt-in**: locally-deleted members are *reported and skipped* unless `--delete` is passed.
- Path mapping is injective and hostile-name-safe (property-tested): case-collisions, `.`/`..`, NULs, and 255-byte names refuse fail-closed at checkout.
- `--decode-scripts` at checkout produces nvim-editable script files that encode back cleanly.

## The single-resource loop (`ign edit`)

```bash
EDITOR=nvim ign edit "MyProject/perspective/MyView/view.json"
```

fetch → decode → `$EDITOR` → encode → push. Designed so an agent can drive it or a human just runs it:

- **Unchanged save = clean no-op** — nothing pushed, never prompted (structural: no staged payload exists to push).
- **Staleness is not overridable** — if the gateway changed since fetch, it refuses (`changed on gateway since fetch`) and there is no `--yes` escape; re-run to fetch fresh. Forcing would clobber concurrent Designer edits.
- **Invalid save = fail-closed** — a JSON-breaking edit refuses and the temp tree is preserved (path printed) for recovery.
- **Zero stdout in every mode** — the editor owns the terminal (documented contract exception).

## Alternative: `ign resource` (no workspace, one resource)

```bash
ign resource list MyProject --json
ign resource get  MyProject "perspective/MyView" --out view.zip
ign resource put  MyProject "perspective/MyView" --file view.zip --yes --json
```

## Live gateway truth in the editor (optional)

`ign lsp` serves tag-path / named-query / provider completions, hover, and diagnostics from TTL-cached gateway truth — never a blocking network call inside an LSP request; a dead gateway degrades to empty diagnostics rather than hanging the editor:

```bash
ign lsp   # register in ignition-nvim as the ignition_live client (PR: ignition-ide-plugins#27)
```

## Guard summary

| Operation | Guard |
|-----------|-------|
| `workspace checkout` / `status` | Read-only (one export GET) |
| `workspace push` | `--yes`; conflicts refuse even with `--yes`; deletions need `--delete` |
| `ign edit` push | Guarded; unchanged = no-op; staleness/fail-closed never force-able |
| `resource put/delete` | `--yes` |
