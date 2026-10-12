---
phase: 20
slug: config-api-client
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-10-12
---

# Phase 20 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Library → gateway HTTP | `DeployApi` calls to config-resource, scan-lock, scan and mode routes | API token header (secret), resource configs, signatures |
| Gateway body → CLI error/output | Gateway response bodies parsed into typed errors and the JSON envelope | Lock holder string, 422 validation text, problem messages (untrusted) |
| Operator scripts → Docker rig | `capture.sh`, `mint-token.sh`, live gate against the disposable 8.3.6 rig | Minted token keys, rig admin password |
| Live gate → any gateway | `live_deploy_api.rs` write operations | Writes to config resources and modes (must never reach production) |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-20-01 | Tampering | `capture.sh` / `mint-token.sh` aimed at a real gateway | high | mitigate | `assert_local` exits 2 before any request (`capture.sh:19-33`); `mint-token.sh` only `docker exec`s into the `ign-deploy-rig` project | closed |
| T-20-02 | Information Disclosure | minted token keys | high | mitigate | umask 077, 0700 state dir outside the repo, 0600 file, path-only stdout (`mint-token.sh:23-69`); `curl -i` headers only; repo and history grep clean | closed |
| T-20-03 | Information Disclosure | rig admin password in `compose.yml` | medium | mitigate | port bound to `127.0.0.1` (`compose.yml:24`); rig destroyed with `down -v` | closed |
| T-20-04 | Information Disclosure | resource signatures in capture blocks | low | accept | see Accepted Risks Log | closed |
| T-20-05 | Denial of Service | rig or held lock left behind | low | mitigate | short holds with `wait_free`; `down -v`; no rig container or volume remains | closed |
| T-20-06 | Information Disclosure | `ResourceSignatureRejected` payload and Display | high | mitigate | variant holds only `name`, `reason`, `endpoint` (`error.rs:748-756`); `deploy_slugs_never_carry_signature_or_problem_text` | closed |
| T-20-07 | Tampering | frozen exit taxonomy | medium | mitigate | `exit_code_mapping_enumerated`, `EXIT_SLUG_LITERALS`, README exit table, `readme_exit_table_agreement` | closed |
| T-20-08 | Spoofing | active-node decision from `role` | medium | mitigate | `active_node` reads only `activity_level` (`redundancy.rs:165-173`); post-failover invariant tests | closed |
| T-20-09 | Denial of Service | `ScanLockHeld.actor` length | low | transfer | transferred to 20-03 classify arm: `LOCK_HOLDER_CAP=256` (`classify.rs:268,292-301`) | closed |
| T-20-10 | Spoofing / Information Disclosure | token handling in `deploy_api.rs` | high | mitigate | single `apply_auth` site (`mod.rs:608-624`) used by every builder; redirects off; header contract test | closed |
| T-20-11 | Repudiation | HTTP 200 taken as scan completion | high | mitigate | `scan_complete` (`scan.rs:171-173`); `scan_trigger_200_but_status_never_advances_is_not_complete` | closed |
| T-20-12 | Tampering | double acquire or scan through retry | high | mitigate | single POST sends; poll wraps GET probes only; `.expect(1)` pins | closed |
| T-20-13 | Denial of Service | holder string or body size | low | mitigate | 256-char cap on the 409 path; shape errors never echo bodies | closed |
| T-20-14 | Tampering | lock decision drift from clock skew | medium | mitigate | clock-free `lock_verdict`; proptest `lock_verdict_ignores_timestamp`; `scan_rs_reads_no_clock` | closed |
| T-20-15 | Denial of Service | client timeout shorter than `acquireTimeout` | medium | mitigate | `ACQUIRE_TIMEOUT_SLACK` 10 s (`deploy_api.rs:47,190-192,690`) | closed |
| T-20-16 | Tampering | path segments in module/type/name | high | mitigate | `validate_segment` refuses before any request; `encode_segment` on name; zero-request contract test | closed |
| T-20-17 | Information Disclosure | signature or `problem.message` echoed in errors | high | mitigate | write guard withholds bodies (`deploy_api.rs:426-507`); `assert_no_leak` tests | closed |
| T-20-18 | Denial of Service | unbounded 422 echo | medium | mitigate | `truncate_api_body` 4096-byte cap on both 422 interpolations; `cap_422_body_is_truncated*` | closed |
| T-20-19 | Repudiation | 2xx `success:false` reported as success | high | mitigate | `send_resource_write` treats `success:false` (and missing) as error | closed |
| T-20-20 | Tampering | duplicate write through retry | high | mitigate | single send, no retry; `resource_write_is_never_retried` | closed |
| T-20-21 | Tampering | regressing the v1.1 EAM exit contract | medium | mitigate | no signature arm in `classify.rs`; deploy-scoped mapping; `eam_contract` green | closed |
| T-20-22 | Tampering | mode name in POST body and paths | medium | mitigate | `validate_segment` before any request; `mode_name_validation_refuses_before_any_request` | closed |
| T-20-23 | Repudiation | `mode_create` 2xx taken as success | medium | mitigate | 2xx confirmed only by `modes()` read-back; WR-01 refusals return before the 2xx path | closed |
| T-20-24 | Spoofing | `deploymentMode` misread | high | mitigate | `normalize_deployment_mode` (`mode.rs:93-95`) over null, `""`, absent, whitespace | closed |
| T-20-25 | Tampering | fake gateway encoding assumptions | medium | mitigate | fake reuses production pure functions; Section E live vs fake diff | closed |
| T-20-26 | Elevation of Privilege | library activating a mode | high | mitigate | no restart/JVM/activate code; `fake_modes_never_activate_and_reject_duplicates` | closed |
| T-20-27 | Tampering / Elevation of Privilege | live gate aimed at production | high | mitigate | `local_url_guard` (`live_deploy_api.rs:86-105`) before any client use; CI tests refuse `10.4.8.15` and look-alikes | closed |
| T-20-28 | Tampering | live gate aimed at another localhost gateway | medium | mitigate | gateway-info name must equal `ign-deploy-rig` before any write | closed |
| T-20-29 | Information Disclosure | token keys in test output or committed log | high | mitigate | names-only output; `Secret(***)` Debug; log and captures scanned clean | closed |
| T-20-30 | Denial of Service | lock or rig left behind | low | mitigate | hold raised 4 s → 12 s by WR-05, still self-expires; Section Z cleanup scan; `down -v` | closed |
| T-20-31 | Repudiation | fake drifting from the gateway | medium | mitigate | Section E asserts live trace == fake trace; drift recorded in captures doc | closed |
| T-20-SC | Tampering | npm/pip/cargo installs | low | accept | see Accepted Risks Log | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-20-01 | T-20-04 | Signatures in capture blocks are opaque per-write content hashes, not credentials, and are dead once the disposable rig is destroyed. Recorded in `20-LIVE-CAPTURES.md`. | 20-01 plan threat model | 2026-10-12 |
| AR-20-02 | T-20-SC | No new package dependencies (no change to any `Cargo.toml` or `Cargo.lock`); the official `inductiveautomation/ignition:8.3.6` image is pinned by tag. | 20-01..20-06 plan threat models | 2026-10-12 |

---

## Unregistered Flags (non-blocking)

Confirmed in code by the auditor, outside any declared mitigation. Address before Phase 23 consumes the write path.

| Flag | Location | Note |
|------|----------|------|
| IN-03 | `config_resources.rs:313,331` | `ResourceUpdate` derives `Debug`, which prints the private signature. No current code logs it; a future `{:?}` would leak it. |
| IN-01 | `classify.rs:292-301`, `deploy_api.rs:705` | Lock holder is length-capped but control characters are not stripped; a hostile gateway could inject terminal sequences into Display and the JSON envelope. |
| IN-07 | `mint-token.sh:45-57` | Operator-supplied token names reach a path and a sourced `tokens.env` unvalidated. Limited to the disposable rig and operator input. |
| — | `deploy_api.rs` `scan_lock_info()` | Direct `scan_lock_info()` does not cap the returned `actor`; only the acquire refusal and enrichment paths do. |

The live gate has not been re-run since review fixes WR-01..WR-06; `20-06-live-gate.log` predates them. Mechanisms for T-20-25, T-20-30 and T-20-31 are green against wiremock and the fake.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-12 | 32 | 32 | 0 | gsd-security-auditor (HEAD b379a94) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-12
