---
status: complete
phase: 20-config-api-client
source: [20-VERIFICATION.md]
started: 2026-10-12T16:46:00Z
updated: 2026-10-12T18:39:33Z
---

## Current Test

[testing complete]

## Tests

### 1. Filtered resource_list against the live 8.3.6 rig
expected: name[eq]=<existing> returns exactly one match; a non-matching name returns zero. Percent-encoded brackets are accepted. Live gate re-run 2026-10-12 (d24e90b): section C asserted one match for name[eq]=<created> and zero for an absent name, sent percent-encoded; PASS C.
result: pass
source: automated (live gate d24e90b; delegated by user)

### 2. Unexercised reads and updates against the live rig (core + production copies present)
expected: resource_singleton reads a singleton type; resource_list_in_collection returns only the named collection's records; an update built with ResourceUpdate::from_record / with_collection (sends collection, enabled, backupConfig) is accepted and changes the intended copy only. Live gate re-run 2026-10-12 (d24e90b): section C asserted per-collection lists in core and production, singleton reads, and a from_record update per copy that rotated only the targeted copy's signature; PASS C.
result: pass
source: automated (live gate d24e90b; delegated by user)

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
