# Quick Task 261102-cef: Deploy tag units to core on redundant pairs (option A2) - Context

**Gathered:** 2026-11-02
**Status:** Ready for planning

<domain>
## Task Boundary

Option A for deploy-tag-redundancy-sync, scoped to redundant pairs (A2). On a target with `nodes.backup`, `ign deploy` writes tag-definition and tag-type-definition units to the `core` config collection instead of the mode collection, so tags can be deployed to pairs again. `deploy_pair_tags_refused` is lifted for units that land in core. Standalone targets keep today's behaviour (tag units in the mode collection).

Why: Ignition 8.3.6 counts tag paths in mode collections in the config-resource sync hash but never sends them (TAG_EXCLUDE_FILTER; tag-config-resource syncs only `core`), so mode-collection tags wedge a pair. Tags in `core` converge (rig controls C1, C3). See .planning/debug/resolved/deploy-tag-redundancy-sync.md.

</domain>

<decisions>
## Implementation Decisions

### D1 Where core routing applies: A2, pairs only
- Route tag-definition / tag-type-definition units to `core` only when the target has `nodes.backup`. Standalone targets are unchanged.
- One routing rule keyed off the target's redundancy (the same signal option C's `Redundancy` uses), applied consistently by plan, apply/transport, ledger, rollback, pull, and verify (including the pair files check reading these units from `core` on gw-b).
- Rationale: fixes the observed Ignition limitation without changing working standalone behaviour or migrating standalone mode tags; easy to drop if Ignition later syncs mode-collection tags.

### D2 Designer edits to ign-managed core tag folders: refuse as drift
- Same rule as every other resource: plan reports drift, apply refuses until the repo is updated or the operator overrides through the existing mechanism. Never silently overwrite.

### D3 Tag units already in a pair's mode collection: move in the same apply
- Write the unit to `core` and delete the mode copy (and its emptied type folder, per option C's cleanup) in one staged commit + scan, so the pair converges in one step.
- `deploy_pair_tags_refused` remains only for cases the move cannot handle (e.g. residual mode files ign cannot delete, as in the 261102-9t6 F2 fix); planner decides the exact residual cases.

### Claude's Discretion
- How collection routing is represented (per-path collection on plan entries vs routing in layout).
- gw-b re-serialization of unary-resource.json in core (parsed-JSON comparison like K2, or writing the gateway's canonical form).
- CoreRelation / ShadowsRemoval semantics for pair tag units living in core.
- Keeping plan_sha256 goldens stable for trees without tag units and for standalone targets.

</decisions>

<specifics>
## Specific Ideas

- Live gate on the Docker 8.3.6 pair rig (fixtures/live-rig-pair/setup-pair.sh up/mint/pair/mode; crates/ignition-core/tests/live_deploy_pair.rs): a tag apply to a pair ends `applied` with both nodes converged and no config-resource snapshot mismatch in gw-b logs; the next non-tag change converges with no resync; a migration case (unit pre-seeded in the mode collection) converges in one apply.
- Constraints: no production calls; no git worktrees; branch feat/deploy-tags-core; never stage .planning/config.json, .planning/research/.cache/, svc-ignition-deploy@52.154.75ace-uat4.ps1; no co-author lines; no production host names or IPs in committed files (public repo); no Cargo.toml version bump; four-place rule + README exit table for any slug change. Do not push or open a PR.
- Gates: cargo fmt --check, workspace clippy -D warnings, cargo test --workspace, cargo xwin clippy windows-msvc.

</specifics>

<canonical_refs>
## Canonical References

- .planning/debug/resolved/deploy-tag-redundancy-sync.md (root cause, fix_options.A costs, rig controls)
- README.md "Tags and redundant pairs"
- .planning/milestones/v1.6-phases/24-redundant-pair/24-LIVE-CAPTURES.md (pair rig, K2 finding)
- .planning/quick/261102-9t6-address-coderabbit-findings-on-pr-34/ (pair_tag_keys delete-set rule)

</canonical_refs>
