---
status: testing
phase: 20-config-api-client
source: [20-VERIFICATION.md]
started: 2026-10-12T16:46:00Z
updated: 2026-10-12T16:46:00Z
---

## Current Test

number: 1
name: Filtered resource_list against the live 8.3.6 rig
expected: |
  resource_list with name[eq]=<existing name> returns exactly that resource, and with a non-matching name returns none.
  This confirms the gateway accepts the percent-encoded brackets reqwest sends (D7 was captured with literal brackets).
awaiting: user response

## Tests

### 1. Filtered resource_list against the live 8.3.6 rig
expected: name[eq]=<existing> returns exactly one match; a non-matching name returns zero. Percent-encoded brackets are accepted.
result: [pending]

### 2. Unexercised reads and updates against the live rig (core + production copies present)
expected: resource_singleton reads a singleton type; resource_list_in_collection returns only the named collection's records; an update built with ResourceUpdate::from_record / with_collection (sends collection, enabled, backupConfig) is accepted and changes the intended copy only.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
