---
phase: 21
slug: deploy-render
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-10-13
---

# Phase 21 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| deploy repo → renderer | `ign deploy render` reads an operator-authored `gateways/<g>/` + `targets/` tree and `deploy.yaml` | YAML/JSON config; may contain secrets (refused as `embedded_secret`) |
| renderer → `--out` dir | Render output written to a local directory (CLI and MCP `deploy_render`) | Rendered gateway config, `render.json` marker (paths, sizes, sha256) |
| renderer → error/stdout/stderr | Findings, envelopes and warnings | Paths and key names only, never values |
| external gw-config → repo fixtures | One-time sanitize of the real gateway config into the committed corpus | Sanitized config (S1-S5 rules), guarded in CI |
| capture.sh → local rig | Evidence capture against a disposable loopback 8.3.6 rig | API token header, synthetic config |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-21-01 | Information disclosure | `fixtures/deploy/corpus/**` | high | mitigate | sanitize.py S1-S5 + self-check; `deploy_fixtures.rs` guard on 3 OSes | closed |
| T-21-02 | Information disclosure | `sanitize.py` stdout | low | mitigate | Per-rule counts and path+rule lines only | closed |
| T-21-03 | Tampering | fixture bytes on Windows checkout | medium | mitigate | `.gitattributes` `-text`/`binary`; `text_fixtures_have_no_carriage_return`, `gitattributes_marks_deploy_fixtures_non_text` | closed |
| T-21-04 | Tampering | `openapi-8.3.6-resource-types.json` drift | low | mitigate | Committed generator; re-run is byte-identical (`cmp`); secret table pinned | closed |
| T-21-04b | Tampering | external source snapshot | medium | mitigate | One-time 195-file + digest identity gate (21-01-SUMMARY); corpus count pinned in `deploy_fixtures.rs` | closed |
| T-21-05 | Spoofing | `capture.sh` BASE | high | mitigate | `assert_local` first statement (capture.sh:24-41); see residual R-01 (WR-08) | closed (L1) |
| T-21-06 | Information disclosure | API token | high | mitigate | mint-token.sh 0600/0700 outside repo; header only; `shcap` token refusal; no token in git | closed |
| T-21-07 | Denial of service | leftover rig / bind dir | medium | mitigate | Bind dir under `$HOME/.cache/ign-deploy-rig/`; `down -v` recorded in 21-02-SUMMARY | closed |
| T-21-08 | Information disclosure | captured fixtures | medium | mitigate | OQ8 secret generated in-container only; guard scans `captured/` | closed |
| T-21-09 | Tampering | Windows-hostile names | medium | mitigate | 0 hostile names in 217 committed fixture files | closed |
| T-21-09b | Repudiation | capture evidence | high | mitigate | `check21` stamp/order/CD-citation enforcement (capture.sh:498-640) | closed |
| T-21-SC | Tampering | `yaml_serde` / `libyaml-rs` | high | mitigate | Lock diff limited to the two packages; both `build = false`, no build.rs | closed |
| T-21-10 | Information disclosure | yaml error text | medium | mitigate | `scrub_values`/`scrub_variant` (yamlio.rs); scrub tests | closed |
| T-21-11 | Denial of service | YAML size / aliases | medium | mitigate | 1 MiB cap before parse; `alias_bomb_is_refused`, `size_cap_boundary` | closed |
| T-21-12 | Tampering | gateway / mode path components | high | mitigate | `component_problem` (deploy/mod.rs:29-43) before path joins | closed |
| T-21-13 | Information disclosure | `EmbeddedSecret` payload | high | mitigate | Details hold paths/findings only (error.rs:875-884) | closed |
| T-21-14 | Tampering | symlinks in tree | high | mitigate | `symlink_metadata` on every entry (tree.rs); finding, never followed | closed |
| T-21-15 | Tampering | `--env` path segment | high | mitigate | Checked before any path (actions/deploy.rs:66-70); contract test | closed |
| T-21-16 | Information disclosure | credentials/network during render | medium | mitigate | `(None, result)` dispatch, no Session; contract tests with no config/env | closed |
| T-21-17 | Repudiation | nondeterministic digest | medium | mitigate | Determinism grep gate clean; sorted entries; golden digest on 3 OSes | closed |
| T-21-18 | Elevation of privilege | MCP-exposed render | low | accept | See Accepted Risks AR-01 | closed |
| T-21-19 | Denial of service | huge / deep JSON | medium | mitigate | 16 MiB cap, depth 128 scan before build; `classify_boundaries` | closed |
| T-21-20 | Tampering | duplicate JSON keys | high | mitigate | Refused at parse; `duplicate_key_refused`, `render_refuses_unparseable_json` | closed |
| T-21-21 | Tampering | writer reformatting bytes | medium | mitigate | Pass-through; rewrite gated to gateway-format; `corpus_render_is_byte_identical` on 3 OSes | closed |
| T-21-22 | Information disclosure | JSON parse error text | low | mitigate | Positions/key names only; `errors_never_echo_values` | closed |
| T-21-23 | Denial of service (lockout) | security/auth resource types | high | mitigate | `DENIED_TYPES` on tree and manifest | closed |
| T-21-24 | Tampering | `config-mode.json` | high | mitigate | Refused at any depth; `config_mode_json_is_refused_anywhere` | closed |
| T-21-25 | Tampering | other env overlay leak | medium | mitigate | Only `modes/<env>/` opened; `other_env_overlays_are_never_read` | closed |
| T-21-26 | Tampering | colliding source paths | medium | mitigate | Injective mapping check naming both origins | closed |
| T-21-27 | Denial of service | dot-named entries | medium | mitigate | Refused at read; `.gitkeep` skipped | closed |
| T-21-28 | Spoofing | uuid collision | low | mitigate | Domain-separated SHA-256, NUL separators | closed |
| T-21-29 | Denial of service | wrong/missing uuid | medium | mitigate | CD1 vector pinned; rule-less types refused | closed |
| T-21-30 | Tampering | opaque `resource.json` rewrite | medium | mitigate | Refused, never rewritten | closed |
| T-21-31 | Tampering | malformed existing uuid | medium | mitigate | Refused naming path, never replaced or echoed | closed |
| T-21-32 | Information disclosure | embedded secret through render | high | mitigate | Embedded scan + 10 schema paths + strict Referenced shape (WR-02 fix e095293) | closed |
| T-21-33 | Information disclosure | secret echoed in output | high | mitigate | `SecretScan` has no value field; canary tests over Display/Debug/`--json`/stdout/stderr | closed |
| T-21-34 | Information disclosure | secret in undescribed field | medium | mitigate | `password`/`secret` key-name rule; see residual R-02 (WR-01) | closed |
| T-21-35 | Denial of service | Windows names/length/case | medium | mitigate | Fail-closed lints, boundary tests, proptests | closed |
| T-21-36 | Tampering | CRLF conversion | medium | mitigate | CR refused in text members; jars exempt | closed |
| T-21-37 | Tampering | `--out` overwrite/delete | high | mitigate | Check-then-write ladder, `create_new`, temp+rename, no-remove gate; see residual R-03 (WR-03) | closed |
| T-21-38 | Tampering | symlink under `--out` | high | mitigate | `symlink_metadata` on out path and entries | closed |
| T-21-39 | Tampering | TOCTOU on `--out` | medium | accept | See Accepted Risks AR-02 | closed |
| T-21-40 | Information disclosure | `render.json` content | low | accept | See Accepted Risks AR-03 | closed |
| T-21-41 | Tampering | partial tree after refusal | medium | mitigate | `write_out` only after `render` Ok; `refused_render_writes_nothing` | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-21-18 | MCP `deploy_render` output is paths and hashes. The original "read-only, no `--out`" premise is stale since 21-09 exposes `--out` over MCP. That surface is covered by T-21-37/38/41. IN-01 (no `confirm` on directory creation, stale "read-only" test message at mcp.rs:874) is open as residual R-11. | plan 21-04 (user-approved plan) | 2026-10-13 |
| AR-02 | T-21-39 | `create_dir`/`create_new` make a racing creator lose. A residual refresh-time race (including IN-03 dir→symlink swap) needs a local attacker who already has write access to the directory. | plan 21-09 (user-approved plan) | 2026-10-13 |
| AR-03 | T-21-40 | `render.json` holds schema version, gateway, env, mode, digest and per-file path/sha256/size. Written only after a successful render, which refuses secrets first. | plan 21-09 (user-approved plan) | 2026-10-13 |
| AR-04 | T-21-34 | Key-name secret rule covers only `password`/`secret` needles on non-empty string values (documented in README and 21-08-SUMMARY). | plan 21-08 (user-approved plan) | 2026-10-13 |

*Accepted risks do not resurface in future audit runs.*

---

## Known Residuals (not accepted, follow-up)

These are review findings that do not block at ASVS L1 / block_on high. They are recorded here so they are not lost and are not yet accepted risks.

| ID | Threat Ref | Finding | Suggested action |
|----|------------|---------|------------------|
| R-01 | T-21-05 (high) | WR-08: `assert_local` cuts the authority only at `/`, so `http://evil.example#@localhost:18290` passes and curl would send the token to `evil.example`. Dev-only script, operator-controlled BASE, rig-only tokens. Would not close at ASVS L2. | One-line fix `authority=${rest%%[/?#]*}` |
| R-02 | T-21-34 | WR-01: a non-string value under a secret-named key (e.g. `{"password":{"value":"…"}}`) is not refused and is not in the README residual statement. | Extend rule or document |
| R-03 | T-21-37 (high) | WR-03: `--out` refresh is not crash-safe. Old `render.json` stays valid over mixed content; failed rename leaves `.ign-render-tmp`; README:2172-2175 claim is false for refresh. No user file is overwritten or deleted. | Fix before Phase 22 reads `render.json` |
| R-04 | T-21-01 | WR-06: sanitize.py writes before its self-check fails (local exposure only; guard blocks commit). | Write to temp, move after check |
| R-05 | T-21-07 | WR-07: `rm -rf` guard in capture.sh:426-431 bypassable with `..` in `IGN_DEPLOY_RIG_RESOURCES`. | Canonicalize before guard |
| R-06 | T-21-06 | IN-11: token on curl command line, visible in `ps`. | Pass header via `-H @file` |
| R-07 | T-21-17 / T-21-28 | WR-05: case-insensitive FS lookups can make spelling differ between macOS and Linux (Linux fails closed). | Compare listed names exactly |
| R-08 | T-21-12 | IN-07: `:` and control characters not refused in components. | Extend `component_problem` |
| R-09 | T-21-35 | IN-05/IN-06: path budget uses only selected env's target; superscript COM/LPT, CONIN$/CONOUT$, Unicode-normalization collisions not caught. | Extend lints |
| R-10 | T-21-32 | IN-08: non-JSON members (`.properties`, `.xml`, `.py`) never secret-scanned. Outside registered scope. | Consider in a later phase |
| R-11 | T-21-18 / T-21-37 | IN-01: MCP `deploy_render` can create directories at any absent path without `confirm`; test message at mcp.rs:874 still says "read-only". | Require `confirm` for `--out` over MCP |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-13 | 44 | 44 | 0 | gsd-security-auditor (opus), ASVS L1; evidence CI run 38483193605 on ff07c85 |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-13
