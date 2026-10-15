---
phase: 22
slug: transport-plan-pull-verify
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-10-15
---

# Phase 22 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Plan |
|----------|-------------|------|
| live data dir → plan engine | Files written by the gateway (or anyone with host access) are read and hashed; names and sizes are untrusted | 22-01 |
| declared repo → plan | `targets/<env>.yaml` chooses which directory is read | 22-01 |
| plan → stdout / MCP | Entries, hashes and warnings reach agents | 22-02 |
| target file → envelope `profile` | The reviewed target names the profile shown in the envelope | 22-02 |
| on-box ledger → delete set | A file on the gateway host decides what plan proposes to delete | 22-03 |
| plan output → Phase 23 apply | `delete_set` and flags drive a later destructive write | 22-03 |
| `--base` directory → plan | A local directory, possibly edited or half-written, claims to be a previous render | 22-04 |
| live file bytes → `--diff` output | Gateway-written content, including secret ciphertext, could reach stdout and MCP clients | 22-04 |
| target YAML → ssh argv | Reviewed but editable repo values become process arguments | 22-05 |
| CI secret env → key file | A private key crosses from the environment to disk | 22-05 |
| remote host → stderr detail | Host-controlled text reaches errors and envelopes | 22-05 |
| remote sftp-server → listings and downloads | The host controls names, sizes, listing text and error text | 22-06 |
| runner temp dir → hashes | Downloaded bytes are hashed locally | 22-06 |
| delete set → filesystem deletes | A computed list removes files from a live collection (Phase 23 caller) | 22-07 |
| verbs and MCP → write half | Agent-reachable code must not reach writes in Phase 22 | 22-07 |
| live gateway files → declared repo (git) | Gateway content, including secrets, could be committed | 22-08 |
| CLI/MCP caller → local repo writes | Pull replaces directories in the operator's repo | 22-08 |
| capture driver → rig | An API token is used against a disposable loopback gateway | 22-09 |
| captured text → repository | Gateway responses are committed as evidence | 22-09 |
| gateway REST (logs, tags route, resources) → verify report | Gateway text and tag values could reach the serialized result | 22-10 |
| live data dir → `files` check | Tag unit bytes are read and compared | 22-10 |
| verify result → stdout / MCP | Findings reach agents and terminals | 22-11 |
| target file → profile resolution | The target names which profile's credential is used | 22-11 |
| runner → WFLWVM01 over ssh | A private key and the host-key pin cross here for the first time | 22-12 |
| capture evidence → repository | Command output from a Windows host is committed | 22-12 |
| runner → Windows PowerShell over ssh | A script runs as `svc-ignition-deploy` on the gateway host | 22-13 |
| Windows sftp-server → listings | Host-formatted output is parsed | 22-13 |
| capture harness → WFLWVM01 production collection | Test code writes and scans a live collection on the capture host | 22-14 |
| prod-like gateway → OQ5 payload | Real configuration is read to size the payload | 22-14 |

---

## Threat Register

Evidence (file:line per threat) is in the 2026-10-15 audit below; status reflects HEAD 0cc801f.

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-22-01 | Tampering | LocalFsTransport walk | high | mitigate | `walk_out` with `symlink_metadata`; symlinks, special files and non-UTF-8 names refused as `transport_failed`; only scoped type and provider directories are opened | closed |
| T-22-02 | Denial of service | huge live tree | medium | mitigate | `READ_MAX_FILE_BYTES`, `READ_MAX_TOTAL_BYTES`, `READ_MAX_ENTRIES` enforced while walking; test `caps_and_hostile_tree` | closed |
| T-22-03 | Tampering | partial read shown as deletions | high | mitigate | Any read error is `transport_failed`; only an absent collection directory reads as empty | closed |
| T-22-04 | Information disclosure | plan result | medium | mitigate | `DeployPlanResult` entries carry paths and sha256 only, never bytes; `--diff` (22-04) owns content output and its withholding | closed |
| T-22-05 | Elevation of privilege | MCP `deploy_plan` | low | accept | Read-only verb with no write path (no `TreeMirror` import, no REST write), unguarded per Decision D | closed |
| T-22-06 | Spoofing | envelope `profile` | low | accept | The name comes from the reviewed target file; plan resolves no credential, so nothing is authenticated as that profile (`plan_cli_needs_no_secret`) | closed |
| T-22-63 | Information disclosure | human and `--json` plan output | medium | mitigate | `render_deploy_plan_human` prints status, key, `core`, `live_in` and hashes only; `deploy_plan_json_stdout_is_the_envelope_only` keeps diagnostics off stdout | closed |
| T-22-07 | Tampering | ledger widening the delete set | high | mitigate | Strict reader (size, UTF-8, duplicate keys, unknown fields, sorted unique paths, gateway and mode match); allowlist and mode-collection filters; `config-mode.json`, dot-names and undescribed types refused | closed |
| T-22-08 | Tampering | ledger copied from another gateway or mode | high | mitigate | `gateway` and `mode` must equal the target's (Pitfall 9); refusal names the location | closed |
| T-22-09 | Tampering | case-insensitive host deletes a rendered file | high | mitigate | Case-folded collisions with rendered paths excluded with a warning; case-variant duplicates in a ledger refused | closed |
| T-22-10 | Denial of service | oversized or deep ledger | medium | mitigate | `LEDGER_MAX_BYTES` before parsing; `node::classify` depth cap | closed |
| T-22-11 | Repudiation | plan reviewed then ledger changed | medium | mitigate | `D` lines bind the delete set into `plan_sha256` (D-08); Phase 23 compares hashes | closed |
| T-22-12 | Information disclosure | ledger refusal text | low | mitigate | Problems name fields and listed paths only, never other file content | closed |
| T-22-13 | Tampering | crash-unsafe or edited `--base` (R-03) | high | mitigate | Re-hash every file; list, per-file hash and digest must match the marker; leftovers and extras refused; identity checked | closed |
| T-22-14 | Tampering | symlink redirection in `--base` | medium | mitigate | `symlink_metadata` on the base and every entry via `walk_out` | closed |
| T-22-15 | Information disclosure | secrets through `--diff` | high | mitigate | Text diff only for `.json` members with no secret-bearing location on either side; everything else hash-only; canary tests over JSON, human, stderr and Debug | closed |
| T-22-16 | Denial of service | huge diffs | low | mitigate | `DIFF_MAX_BYTES` per entry with a truncation marker | closed |
| T-22-SC | Tampering | `similar` crate | low | mitigate | Already in `Cargo.lock` at 3.2.0 (RESEARCH package legitimacy audit: OK); no new lock entries, verified by `git diff --stat Cargo.lock` | closed |
| T-22-17 | Elevation of privilege | `host`/`user` read as ssh options (`-oProxyCommand`) | high | mitigate | Leading `-`, whitespace, control characters and `@` in user refused; `--` before the destination in every argv | closed |
| T-22-18 | Tampering | command injection through paths in sftp batches | high | mitigate | argv only; `safe_remote_path` refuses quote, backslash, control and glob characters; batch lines built only from validated inputs | closed |
| T-22-19 | Spoofing | host-key MITM or TOFU | high | mitigate | Repo known_hosts copy, `StrictHostKeyChecking=yes`, global file set to the null device, `UpdateHostKeys=no`, `VerifyHostKeyDNS=no`; `ssh -G` contract test | closed |
| T-22-20 | Information disclosure | private key on disk, in argv or in logs | high | mitigate | 0600 file in a 0700 temp dir deleted on drop, Windows owner-only ACL, only the path in argv, manual `Debug`, canary-style `key_never_echoed` test | closed |
| T-22-21 | Denial of service | hung ssh (stdin, dead VPN) | medium | mitigate | stdin piped and closed, `ConnectTimeout`, `ServerAlive*`, per-call timeout with kill-on-drop | closed |
| T-22-22 | Information disclosure | host stderr echoed | medium | mitigate | `scrub_stderr` tail, cap, temp path removal, control characters stripped | closed |
| T-22-23 | Tampering | known_hosts swapped through a symlink | medium | mitigate | `read_repo_file` refuses symlinks and symlinked parents; the copy is what ssh reads | closed |
| T-22-24 | Tampering | silent partial read (error hidden by `-` prefix) | high | mitigate | No `-` prefix; existence-first sessions; any exit or stderr is `transport_failed`; downloaded dirs checked against listings | closed |
| T-22-25 | Tampering | hostile names injected into later batches | high | mitigate | Remote names never interpolated; only validated segments, providers and fixed paths enter batch lines | closed |
| T-22-26 | Tampering | hostile names in a recursive download (`..`, `/`) | medium | mitigate | The sftp client refuses such names; local `walk_out` refuses symlinks, special files and non-UTF-8 names; listing parser refuses `.`, `..`, `/` | closed |
| T-22-27 | Denial of service | huge remote files fill the runner disk | low | accept | Caps are enforced after download; the host is the deploy target the operator controls; disk fill needs host write access | closed |
| T-22-28 | Tampering | remote symlink inside a resource dir is skipped by `get -R` | low | accept | Verified behaviour pinned by `sftp_get_skips_symlinks_documented`; gateways do not write symlinks under `config/resources`; Windows reparse-point behaviour is a C7a capture item (22-12) | closed |
| T-22-29 | Information disclosure | downloaded config left on the runner | medium | mitigate | Per-read `TempDir` deleted on drop | closed |
| T-22-30 | Elevation of privilege | a verb or MCP tool reaching `commit` | high | mitigate | Trait split (`for_node` returns `TreeReader` only) plus `write_half_unreachable` source scan over actions, main and mcp | closed |
| T-22-31 | Tampering | deleting beyond the bounded list | high | mitigate | Only delete-set files are removed; `config-mode.json`, dot-names, short paths and staged paths refused before any I/O; emptied-dir removal stops at `<module>/<type>` | closed |
| T-22-32 | Tampering | foreign files inside the scanned collection (Designer `NameInvalidException`) | high | mitigate | No temp or dot-named names in the collection; staging root inside `config/resources` refused; walk test after both modes | closed |
| T-22-33 | Tampering | symlink swap at a delete target | medium | mitigate | `symlink_metadata` check before `remove_file`; sftp `rm` removes the link itself, never its target | closed |
| T-22-34 | Repudiation | partial commit after an I/O error | medium | transfer | Phase 23 wraps `commit` in the scan-lock with a snapshot and one `restore_snapshot` path (ROADMAP Phase 23 constraints) | closed |
| T-22-35 | Information disclosure | Embedded secrets written into git | high | mitigate | Render fixpoint skips every unit with an Embedded or plaintext secret finding; reasons carry paths only; canary test over files, stdout and stderr | closed |
| T-22-36 | Tampering | overwriting a hand-edited tree | high | mitigate | `confirmation_required` before any read or write unless `--yes`; only three subtrees replaced; manifest and other overlays kept; GUARDED_OPS plus MCP `confirm` | closed |
| T-22-37 | Tampering | symlink in the repo redirecting removal or writes | high | mitigate | Symlink walk before any removal; `create_new` writes; `require_repo_dir` on `gateways/` | closed |
| T-22-38 | Tampering | hostile live names becoming repo paths | medium | mitigate | Render's name lints and the fixpoint drop such units; non-UTF-8 names fail the read | closed |
| T-22-39 | Elevation of privilege | MCP pull creating directories without `confirm` when the target is absent or empty | low | accept | D-11 guards only a non-empty overwrite; the write is confined to `gateways/<name>/` of the caller's own `--repo` (21-SECURITY R-11 pattern, documented in README) | closed |
| T-22-40 | Spoofing | `capture22.sh` BASE redirect (R-01 pattern) | high | mitigate | `assert_local` with the authority cut at `/`, `?` or `#`; the `evil.example#@localhost` probe in verify exits 2 | closed |
| T-22-41 | Information disclosure | API token in evidence | high | mitigate | Token in a 0600 env file outside the repo, sent as a header only, never printed; acceptance greps the captures for a token header | closed |
| T-22-42 | Information disclosure | rig config in evidence | low | accept | Disposable rig with synthetic resources; CV2 records JSON paths, not values | closed |
| T-22-43 | Denial of service | leftover rig or bind dir | medium | mitigate | Bind dir under `$HOME/.cache`; `down -v` recorded | closed |
| T-22-44 | Information disclosure | log messages and tag values in findings | high | mitigate | `LogLine.message` never serialized; findings carry keys, paths, quality codes, timestamps, logger names; `no_message_or_value_echo` canary test | closed |
| T-22-45 | Tampering | verify accidentally writing | high | mitigate | Only read methods are called; `ForbidWrites` double; transport is `TreeReader` only | closed |
| T-22-46 | Repudiation | false green from an unevaluated check | high | mitigate | Unavailable sources fail their check; absent probes are `warn`; unreachable gateway is an error, not a verdict | closed |
| T-22-48 | Denial of service | probe polling | low | mitigate | `PROBE_DEADLINE` 30 s at `PROBE_INTERVAL` 1 s, bounded | closed |
| T-22-47 | Spoofing | profile chosen by an edited target | medium | mitigate | Profile name resolved through the normal secret chain (`named_profile_client`); the target is reviewed in git; the envelope names the profile used | closed |
| T-22-49 | Elevation of privilege | MCP `deploy_verify` | low | accept | Read-only, unguarded per Decision D | closed |
| T-22-64 | Information disclosure | human and `--json` verify output | high | mitigate | `render_deploy_verify_human` prints finding code, subject and detail only (22-10 keeps values out of findings); `verify_cli_human_lines` seeds a log-message canary; `deploy_verify_json_stdout_is_the_envelope_only` keeps diagnostics off stdout | closed |
| T-22-65 | Repudiation | red verdict mistaken for success by exit code | medium | mitigate | Exit 6 set after the envelope (`verify_cli_red_exits_6`); README dated exception documents the rule (DSURF-04) | closed |
| T-22-50 | Spoofing | first contact with WFLWVM01 (TOFU) | high | mitigate | Pin only after the keyscan fingerprint equals the console fingerprint from the human; `StrictHostKeyChecking=yes` thereafter | closed |
| T-22-51 | Information disclosure | private key in evidence or on disk | high | mitigate | Key from env into a 0700 temp dir removed on exit; command lines print `<key>`; acceptance greps for key headers | closed |
| T-22-52 | Tampering | capture probes left inside the live collection | medium | mitigate | Probe directory removed in the same session; scan status recorded before and after; scratch work confined to the account's home | closed |
| T-22-53 | Elevation of privilege | over-privileged service account | medium | mitigate | Account created non-admin with read-only data-dir access; C6 records actual ACLs instead of granting more | closed |
| T-22-54 | Information disclosure | host names and ACLs in evidence | low | accept | Internal capture host; no secrets in ACL listings | closed |
| T-22-55 | Elevation of privilege | PowerShell injection through paths | high | mitigate | Fixed script bodies; parameters only as a base64 JSON blob decoded in-script; no interpolation; budget enforced before spawn | closed |
| T-22-56 | Tampering | Windows path normalisation mistakes (UNC, drive-relative) | medium | mitigate | `windows_sftp_root` refuses UNC, relative and quote-bearing paths; vectors from captured blocks | closed |
| T-22-57 | Repudiation | CLIXML or progress noise read as success | medium | mitigate | Only a single `IGN-RESULT:` line counts; stderr bytes and non-zero exits are `transport_failed` | closed |
| T-22-58 | Tampering | live write test touching the real collection | high | mitigate | Write-half live test bound to a scratch data dir under the account's home, never the production collection | closed |
| T-22-59 | Tampering | harness leaving WFLWVM01 changed | medium | mitigate | Pre-run hash listing; payload removed through the delete set; post-run hashes asserted equal | closed |
| T-22-60 | Information disclosure | prod-like config in the payload | high | mitigate | Payload comes from `deploy pull`, which skips secret-bearing units; nothing from the payload is committed, only counts and timings | closed |
| T-22-61 | Elevation of privilege | Defender exclusion widening attack surface | low | accept | Exclusion scoped to the data and staging dirs on a capture host by an administrator; the production recommendation is recorded for the user, not applied | closed |
| T-22-62 | Denial of service | scan storms on the capture host | low | accept | Bounded to 10 iterations per run on a capture host | closed |
*Status: open · closed · open — below high threshold (non-blocking)*
*Disposition counts: 55 mitigate, 10 accept, 1 transfer (T-22-34 → Phase 23 fault-injection and `restore_snapshot`, ROADMAP.md:361,366). T-22-27 and T-22-28 were planned as accept and are implemented as mitigations.*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-22-05 | MCP `deploy_plan` is read-only. The plan arm reaches only `for_node` and a `TreeReader` (main.rs plan arm; `deploy_plan_is_an_unguarded_catalog_tool`). | plan 22-02 (user-approved plan) | 2026-10-15 |
| AR-02 | T-22-06 | The envelope `profile` is a label from the reviewed target. Plan resolves no credential (`plan_cli_needs_no_secret`). | plan 22-02 (user-approved plan) | 2026-10-15 |
| AR-03 | T-22-27 | Runner disk fill from huge remote files. Now also mitigated: total cap before each chunk, dir-size watch, per-file cap and size equality (sftp.rs). | plan 22-06 (user-approved plan) | 2026-10-15 |
| AR-04 | T-22-28 | `get -R` skips symlinks. Now also mitigated: symlinks refused from the listing and `GetRecursive` never built in product code. | plan 22-06 (user-approved plan) | 2026-10-15 |
| AR-05 | T-22-39 | MCP pull into an absent or empty `gateways/<name>/` runs without `confirm`. D-11 guards only a non-empty overwrite, and the write stays inside the caller's own `--repo` (README.md:2895-2899). | plan 22-08 (user-approved plan) | 2026-10-15 |
| AR-06 | T-22-42 | Rig config in evidence. Disposable rig with synthetic resources. CV2 records paths, not values. | plan 22-09 (user-approved plan) | 2026-10-15 |
| AR-07 | T-22-49 | MCP `deploy_verify` is unguarded because it is read-only (Decision D; README.md:2973-2977). | plan 22-11 (user-approved plan) | 2026-10-15 |
| AR-08 | T-22-54 | Host names and ACLs in evidence. Internal capture host, no secrets. A block carrying the client address was removed. | plan 22-12 (user-approved plan) | 2026-10-15 |
| AR-09 | T-22-61 | Defender exclusion scoped to the capture host and set by an administrator. 22-14 recommends no exclusion. The host resource group is deleted. | plan 22-14 (user-approved plan) | 2026-10-15 |
| AR-10 | T-22-62 | Scan storms on the capture host, bounded by `IGN_OQ5_ITERS` (default 10). | plan 22-14 (user-approved plan) | 2026-10-15 |

*Accepted risks do not resurface in future audit runs.*

---

## Residuals (non-blocking)

| ID | Threat Ref | Residual |
|----|------------|----------|
| R-01 | T-22-20, T-22-29 | After SIGINT or SIGTERM the `key_env` key (0600 in a 0700 dir) and downloads stay on disk until a later `RemoteShell` sweeps `ign-ssh-*` dirs at least 6 h old (review WR-01, fix c74a79b). The ssh and sftp children inherit the `key_env` variable, and the `-i` path is not `%`-escaped (review IN-05). |
| R-02 | T-22-36 | Deterministic pull failures are refused before the first delete (review CR-01, fix 83a3998). ENOSPC, EIO or a signal mid-write can still leave a partial repo tree. The staging-plus-rename swap is not built. |
| R-03 | T-22-57 | On exit 0 the PowerShell path fails only on CLIXML error records and tolerates other stderr (C7a-6). Success still requires exactly one `IGN-RESULT:` stdout line. The strict any-stderr rule applies to sftp `require_success` only. |
| R-04 | T-22-02 | The local walk has no depth bound (review IN-08). Breadth, file and byte caps apply. |
| R-05 | T-22-44 | Unchecked i64 subtraction in `log_window_start` on gateway-supplied values (review IN-02). |
| R-06 | T-22-28, T-22-30 | `SftpOp::GetRecursive` and the `#[doc(hidden)] pub RemoteShell::local_sftp_server` seam remain with test-only callers (review IN-03). |
| R-07 | T-22-37 | Windows only: `open_regular_file` and the non-unix `write_pulled_fs` keep the documented intermediate-directory race. |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-15 | 66 | 66 | 0 | gsd-security-auditor (opus), ASVS L1, block_on high, HEAD 0cc801f. 492 security-relevant tests passed in an isolated target dir. No unregistered SUMMARY threat flags (22-13 `ssh-remote-exec` maps to T-22-55). |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-15
