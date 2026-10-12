# Phase 21: Deploy Render - Context

**Gathered:** 2026-10-12 (plan-phase, from research open questions; discuss-phase skipped)
**Status:** Ready for planning

<domain>
## Phase Boundary

Offline, deterministic `ign deploy render` of a `gateways/<name>/` tree for one environment, per ROADMAP Phase 21 goal, success criteria and constraints (those are locked). This file records only the decisions taken on the research open questions.
</domain>

<decisions>
## Implementation Decisions

- **D-01:** The CR-byte refusal (DRND-08, success criterion 4) applies to text members only (JSON, YAML, CSS, other text). Binary members such as `.jar` are exempt. A test must prove a jar containing 0x0D is NOT refused.
- **D-02:** A type directory not on the manifest allowlist is refused as `deploy_tree_invalid` naming the directory (fail closed, no skip).
- **D-03:** Name-based `attributes.uuid` is derived with `sha2` (already a dependency) and stamped as UUID version 8. No `sha1` or `uuid` crate. A live check that 8.3.6 loads a v8 uuid is required before the derivation is frozen.
- **D-04:** NTFS behaviour for hostile names (C5 on a Windows host) is deferred to Phase 22. Phase 21 captures the gateway's Linux encoding only, and render stays fail-closed on Windows-illegal names.

### Claude's Discretion
- `config-mode.json` and dot-named components are refused, except `.gitkeep`, which is skipped (research OQ-D).
- YAML input is capped at 1 MiB, and anchors/aliases are allowed (research OQ-E).
- The `--out` write target is optional and uses a clobber ladder (research safety note).
</decisions>

<deferred>
## Deferred Ideas

- NTFS hostile-name behaviour capture → Phase 22 (D-04).
</deferred>
