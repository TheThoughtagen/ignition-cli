---
status: resolved
trigger: "CI flake: contract_mcp::mcp_tools_call_write_verbs_unknown panics at crates/ignition-cli/tests/contract_mcp.rs:217 'no response with id 32 arrived within 15s - protocol death or starvation'."
created: 2026-10-19
updated: 2026-10-19
---

# Debug: MCP write-verb test times out in CI

## Symptoms

DATA_START
- expected: `mcp_tools_call_write_verbs_unknown` (contract_mcp.rs:1136, added in 9e2f676, plan 23-14) passes on every CI leg. While a slow `status` call (wiremock, 3 s delay) is in flight as id 30, `tools/call` for `deploy_apply` (id 31) and `deploy_rollback` (id 32) must be answered promptly as unknown tools.
- actual: on GitHub Actions it times out waiting for id 32 (15 s budget in `expect_response`, contract_mcp.rs:198-217). Seen: macOS run (phase 23, first CI), ubuntu run 39219142462 attempt 1, ubuntu run 39263180512 attempt 1, macOS run 39263180512 attempt 2 -> 3 of the last 4 CI attempts. Passes locally in full workspace runs. Fail-fast then cancels windows-latest, which blocks CI proof of phase 23's sftp changes.
- errors: `thread 'mcp_tools_call_write_verbs_unknown' panicked at crates/ignition-cli/tests/contract_mcp.rs:217:68: no response with id 32 arrived within 15s - protocol death or starvation`. Test binary finished in ~30 s.
- timeline: test exists since 9e2f676 (23-14). Intermittent since first CI run of phase 23.
- reproduction: CI only so far. `gh run view 39263180512 --log-failed` has the logs. Try locally under load / with constrained worker threads (e.g. taskset/CPU limit, `--test-threads`), and read the MCP server's request dispatch (is a tools/call for an unknown tool serialized behind the in-flight slow call? does an unknown-tool error take a path that waits on the gateway client, a profile load, or a lock?).
DATA_END

## Constraints

- Decide from evidence whether this is a test-harness timing problem or a real server defect (a request blocked behind another). Do not just raise the timeout unless the evidence shows the server is correct and the budget is wrong; say which.
- TDD for a server fix: a RED test that reproduces the blocking deterministically.
- No push, no co-author trailer, one commit per step, no worktree, never print secrets/tokens, use /bin/rm (shell rm is wrapped). Do not touch ~/.config/ignition-cli or the Keychain; wrap any `ign` in `timeout 120 ... </dev/null`.
- Do not commit .planning/config.json or .planning/research/.cache/.
- If the permission system denies an action, stop and report it; do not work around it.
- After the fix: cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check, `cargo build -p ignition-cli --no-default-features`. Run long ones in the background.

## Current Focus

bug_class: Heisenbug (scheduling-order dependent; deterministic once the order is fixed)
known_pattern_candidate: none (knowledge-base.md has no MCP/ordering entry)
verdict: TEST-HARNESS DEFECT, server correct. Raising RESPONSE_BUDGET_SECS would NOT help: the response is already consumed and discarded, so a larger budget only makes the failure slower.
hypothesis (CONFIRMED): (H3) test-harness defect. `ScriptedClient::expect_response` (contract_mcp.rs:198-223) DISCARDS every message whose id does not match. Ids 31 and 32 are two tokio::spawn'd tasks on a multi-thread runtime, so their responses may legally arrive 32-before-31. When that happens, `expect_response(31)` reads and drops 32's response, then `expect_response(32)` waits on a channel that will never carry it again -> 15 s timeout. The server is correct.
test: (a) replay the exact request sequence against the real `ign mcp serve` binary many times, constrained via TOKIO_WORKER_THREADS / CPU load, and record raw response arrival order; (b) count runs where 32 arrives before 31; (c) confirm no run shows a response for 31/32 delayed behind the slow call.
expecting: some runs show order [.., 32, 31, ..] with both answered in <100 ms; zero runs where 31 or 32 is late. That confirms H3 and refutes H1.
next_action: Session manager runs its independent verification of fix commit 8c424f1 (its earlier gates_sm.sh contract_mcp result is INVALID: it executed my M2 mutant binary, see the INCIDENT evidence entry; rerun contract_mcp). On confirmation: archive to resolved/, commit the debug file, append the knowledge-base entry from the Prevention block below. Nothing else is pending in this session.

tdd_checkpoint:
  red_commit: "38e3418a48d583bcdf1c592fb59e81359b9c7224"
  test_file: "crates/ignition-cli/tests/contract_mcp.rs"
  test_name: "inbox_keeps_an_out_of_order_reply_for_a_later_wait"
  companion_tests:
    - "inbox_recv_next_returns_set_aside_replies_first_in_arrival_order"
    - "inbox_expect_silence_fails_while_a_reply_is_set_aside"
  status: "green"
  green_commit: "8c424f1b5372f3a1a4e5757212c28f99981630d9"
  green_output: "cargo test -p ignition-cli --test contract_mcp inbox_ -> 3 passed; 0 failed; 13 filtered out"
  failure_output: |
    thread 'inbox_keeps_an_out_of_order_reply_for_a_later_wait' panicked at crates/ignition-cli/tests/contract_mcp.rs:287:68:
    no response with id 32 arrived within 1s — protocol death or starvation (see stderr for the server's trace log by rerunning with stderr inherited)
    ---- inbox_recv_next_returns_set_aside_replies_first_in_arrival_order: assertion `left == right` failed: arrival order: {..."id":34...}  left: Some(Number(34))  right: Some(String("b"))
    ---- inbox_expect_silence_fails_while_a_reply_is_set_aside: note: test did not panic as expected at crates/ignition-cli/tests/contract_mcp.rs:371:4
    test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 13 filtered out; finished in 1.01s

reasoning_checkpoint:
  hypothesis: "mcp_tools_call_write_verbs_unknown times out on id 32 because ScriptedClient::expect_response discards any message whose id does not match the awaited one; the server answers the two concurrently spawned unknown-tool calls in either order, and when 32 lands before 31, expect_response(31) throws 32's response away, so expect_response(32) can never succeed."
  confirming_evidence:
    - "Instrumented harness, TOKIO_WORKER_THREADS=3, 20 runs: 8/8 failures logged `expect_response(31) DROPPED a message with id 32`; 12/12 passes logged no drop (perfect correlation)."
    - "Replay against the real binary: 29/120 multi-worker runs answered 32 before 31, 0/160 runs answered 31 or 32 later than 24 ms. The server never blocks them behind the slow call."
    - "Differential on the real test: TOKIO_WORKER_THREADS=1 gives 0/30 failures (FIFO), 3 workers 10/30, default 4/30. A server-blocking cause (H1) would fail MORE with one worker."
    - "CI signature: always id 32, never 31/30; FAILED stamped 15.05-15.31 s after the binary started, i.e. 31 arrived at once and the whole budget was spent waiting for an already-discarded 32."
  falsification_test: "If a failing run showed no DROPPED-32 diag line, or if 31/32 were ever answered late (>2 s) in the replay, or if TOKIO_WORKER_THREADS=1 still failed, H3 would be wrong. None of those was observed."
  fix_rationale: "Make the harness keep out-of-order responses: expect_response stashes non-matching messages (e.g. a RefCell<VecDeque<Value>> pending buffer on ScriptedClient) and checks the stash before reading the channel; recv_next pops the stash first (keeps arrival order); expect_silence fails if the stash is non-empty. That fixes the cause (lost messages), not the symptom (timeout), and makes the helper match its own doc comment ('legal out-of-order delivery'). No server change: out-of-order responses are legal JSON-RPC and are the very property that keeps ping from starving (mcp.rs:279-285)."
  blind_spots: "contract_lsp.rs::expect_response has the same discard-on-mismatch shape (line 154). It is not live today: lsp-server's loop is synchronous and answers in request order, and those tests interleave only notifications. It becomes a latent flake if an LSP test ever has two outstanding requests. Not verified on the Windows leg (fail-fast cancelled it before this test ran); the mechanism is OS-independent."
  candidate_causes:
    - "code (test harness): expect_response discards non-matching ids. CONFIRMED."
    - "code (server): tools/call serialized / lock held across gateway call. REFUTED (spawned per call, unknown tool returns before any I/O; replay 0/160 late)."
    - "environment (CI): fewer cores / contention. CONTRIBUTING TRIGGER only: it raises the 32-before-31 rate (3 workers: 10/30 vs default: 4/30) but is not a defect."
    - "config (budget): RESPONSE_BUDGET_SECS=15 too tight. REFUTED: the message is gone, so no budget can rescue it."
  and_gate: "yes, two conditions must coincide: (a) the harness drops non-matching ids [the defect] AND (b) the server answers 32 before 31 [legal, nondeterministic, about 25% with >=2 workers]. (b) is correct server behavior and must stay, so the single root cause to fix is (a)."

## Evidence

- timestamp: 2026-10-19
  checked: crates/ignition-cli/src/mcp.rs serve() lines 286-387 and execute() lines 452-470
  found: The reader loop runs on the block_on thread; every tools/call is `tokio::spawn`ed (no lock, no semaphore, no shared state besides an Arc<catalog> and an unbounded mpsc to the single writer task). For an unknown tool, `execute` returns -32601 at the catalog `find` (line 467-470), before any config load, Session::resolve, keyring, or gateway work. main.rs:490 builds a default multi-thread runtime (workers = CPU count; 3 on macos-latest arm64, 4 on ubuntu-latest).
  implication: Nothing serializes 31/32 behind 30. Two back-to-back spawned tasks on a multi-thread runtime have no ordering guarantee; 32 can finish before 31. The server's own doc comment says out-of-order responses are legal (JSON-RPC id matching).

- timestamp: 2026-10-19
  checked: crates/ignition-cli/tests/contract_mcp.rs expect_response (198-223) and mcp_tools_call_write_verbs_unknown (1136-1220)
  found: expect_response loops on `lines.recv_timeout`; a message with a different id hits the comment "A different id's response: legal out-of-order delivery - keep waiting" and is DROPPED (not stashed). The test sends 30 (slow status), 31, 32 back to back, then calls expect_response(31) then expect_response(32) sequentially.
  implication: If 32's response arrives first, expect_response(31) consumes and discards it; expect_response(32) then can only time out. The harness claims to tolerate out-of-order delivery but loses the out-of-order message.

- timestamp: 2026-10-19
  checked: CI logs, run 39219142462 attempt 1 (ubuntu), 39263180512 attempt 1 (ubuntu), 39263180512 attempt 2 (macos) via `gh run view --log-failed`
  found: All three fail on id 32, never 31 or 30. Time from "running 13 tests" to "FAILED": 15.05 s, 15.09 s, 15.31 s. So id 31 was received within ~0.05-0.3 s of the binary starting and the full 15 s budget was then spent waiting for 32.
  implication: 31 was answered promptly, so the server was not blocked at that moment. Under H1 (32 queued behind the slow status call) 32 would arrive once status finishes: status makes at most 3 sequential gateway requests (inspect.rs:116-119) at 3 s each, so at most ~9 s, inside the 15 s budget. H1 does not predict a timeout, and H1 would have to block 32 but not 31 although both take the identical code path. The signature (exactly one id lost, always the second, full budget burned) matches H3.

- timestamp: 2026-10-19
  checked: Replay of the exact request sequence (initialize, initialized, tools/list, then 30 status / 31 deploy_apply / 32 deploy_rollback back to back) against the real debug `ign mcp serve` binary, with a 3 s / `{}` fake gateway, IGNITION_LOG=trace, 40 runs per config. Script: scratchpad replay/replay.py. Config varied via TOKIO_WORKER_THREADS.
  found: workers=1: 0/40 flipped. workers=2: 9/40 runs answered 32 BEFORE 31. workers=3: 10/40. default (10 cores): 10/40. In all 160 runs, 31 and 32 were both answered within 0.3-24 ms of the send; 0 runs had 31 or 32 later than 2 s. Response 30 always arrived at ~3.01-3.06 s (after the slow gateway).
  implication: CONFIRMS H3 mechanism: the server answers 32 before 31 in about a quarter of runs whenever there is more than one worker thread (CI: 3-4). REFUTES H1: the server never held an unknown-tool reply behind the in-flight slow call. With one worker the spawn queue is FIFO, so no flips, which is also why the order looks stable on a lightly loaded machine.

- timestamp: 2026-10-19
  checked: The REAL test, `contract_mcp-<hash> --exact mcp_tools_call_write_verbs_unknown`, looped 30x per config on this Mac (10 cores) at HEAD 7523963 (scratchpad loop_test.sh; TOKIO_WORKER_THREADS is inherited by the spawned server).
  found: default workers: 4/30 FAIL. TOKIO_WORKER_THREADS=3 (macos-latest core count): 10/30 FAIL. Every failure is exactly "no response with id 32 arrived within 15s", the CI message.
  implication: The CI flake reproduces locally with no CI involved. The failure rate rises as the worker count falls toward the CI runner core counts. The test harness sends each frame as two write syscalls (json, then "\n"), which gives task 31 a small head start and makes the flip rarer in the real test than in the single-write replay. That is why a full local run usually passes.

- timestamp: 2026-10-19
  checked: Differential: the same real-test loop, 30x, TOKIO_WORKER_THREADS=1.
  found: 30/30 PASS.
  implication: With one worker the spawned tasks run FIFO, so 31 is always answered first and nothing is dropped. H1 (server blocking) would predict the SAME OR MORE failures with one worker; the opposite was observed.

- timestamp: 2026-10-19
  checked: Direct observation: temporary `eprintln!("[diag] expect_response({id}) DROPPED a message with id ..")` in expect_response's mismatch branch, 20 runs with TOKIO_WORKER_THREADS=3 and --nocapture. Instrumentation reverted afterwards with `git checkout -- crates/ignition-cli/tests/contract_mcp.rs` (tree clean).
  found: 8 FAILED runs, each logging exactly `expect_response(31) DROPPED a message with id 32`, then `expect_response(32) DROPPED a message with id 30` (the status reply at ~3 s). 12 ok runs, zero drops.
  implication: CONFIRMED. Every failure is the harness throwing away 32's response while waiting for 31. A second loss follows: the id-30 reply is also discarded while waiting for 32, so the test's final `expect_response(30)` (line 1209) would time out too even with a bigger budget.

- timestamp: 2026-10-19
  checked: TDD RED. Extracted ScriptedClient's recv_next / expect_response / expect_silence into `struct Inbox { lines: Receiver<Value> }` (ScriptedClient keeps `inbox: Inbox` and delegates, call sites unchanged). Moved bodies diffed against HEAD 7523963: byte-identical for all three. Added three harness tests that feed replies through a plain mpsc channel (sender kept alive so a failure is the lost reply, not a disconnect), 1 s budget: inbox_keeps_an_out_of_order_reply_for_a_later_wait (fed 32,31,30; waits 31,32,30 = the CI order), inbox_recv_next_returns_set_aside_replies_first_in_arrival_order (fed "b",33,31,34; wait 31; recv_next must give "b",33,34), inbox_expect_silence_fails_while_a_reply_is_set_aside (#[should_panic(expected = "quiet window")]; fed 32,31; wait 31; expect_silence(50)).
  found: `cargo test -p ignition-cli --test contract_mcp inbox_` -> 0 passed, 3 failed, 1.01 s. (1) panics at contract_mcp.rs:287:68 "no response with id 32 arrived within 1s" (the CI message, 1 s budget): expect_response(31) returned 31 on its second read, so the first-read 32 was consumed and dropped, and the wait for 32 then also dropped 30. (2) recv_next returned 34, not "b": "b" and 33 were dropped by the wait for 31. (3) "test did not panic as expected": 32 was dropped, so the quiet window passed over it.
  implication: RED for the right reason, deterministic and independent of tokio scheduling. Committed alone as 38e3418 (test(23)), contract_mcp.rs only.

- timestamp: 2026-10-19
  checked: Behavior neutrality of the extraction. `cargo test -p ignition-cli --test contract_mcp -- --skip inbox_`; then the real write-verbs test looped 12x per config from the built binary; `cargo fmt --all -- --check`; `cargo clippy -p ignition-cli --tests -- -D warnings`.
  found: 11 passed, 1 ignored (uv oracle), 1 failed: mcp_tools_call_write_verbs_unknown with the original "no response with id 32 arrived within 15s" (the natural flake). Loop: TOKIO_WORKER_THREADS=1 12/12 pass (HEAD baseline 30/30); default workers 10 pass / 2 id-32 timeouts (HEAD baseline 4/30 fail). fmt exit 0, clippy clean.
  implication: The extraction changed no behavior: the other tests pass and the flake keeps its HEAD signature and rate. The write-verbs failure in the suite run is the bug under repair, which the GREEN step must remove.

- timestamp: 2026-10-19
  checked: TDD GREEN. Inbox gained `set_aside: RefCell<VecDeque<Value>>`. expect_response first claims a set-aside reply with the same `message.get("id") == Some(id)` equality, and pushes any non-matching reply it reads to the back instead of dropping it. recv_next pops the set-aside front before reading the channel. expect_silence panics ("expected a quiet window ... still unclaimed") while the set-aside is non-empty. Diff confined to crates/ignition-cli/tests/contract_mcp.rs (Inbox struct, Inbox::new, the three Inbox methods, two `use` lines). The three RED tests were not modified. No change to src/mcp.rs, RESPONSE_BUDGET_SECS, or QUIET_WINDOW_MS.
  found: `cargo test -p ignition-cli --test contract_mcp inbox_` -> 3 passed, 0 failed, 13 filtered out, 0.00 s.
  implication: GREEN for the three harness rows that were RED at 38e3418.

- timestamp: 2026-10-19
  checked: Whether contract_lsp.rs:142-165 `LspClient::expect_response` (same drop-on-mismatch shape) should change in this fix. Read lsp.rs:100-125 and the LSP tests' receive sequences.
  found: (1) The LSP server is one synchronous `crossbeam_channel::select!` loop that sends each Response inline (lsp.rs:109-123), so two outstanding LSP requests are answered in request order: the response/response flip that broke MCP cannot happen. (2) What LspClient::expect_response drops are publishDiagnostics notifications, and the data-plane test DEPENDS on stale publishes being discarded: it lets the completion poll drop them, then drains the channel directly (contract_lsp.rs:578-587, reading `client.messages`) before the didChange whose publish it asserts. A set-aside buffer there would hand expect_publish a stale pre-edit publish ahead of the one under test and would bypass that drain.
  implication: Leave contract_lsp.rs unchanged. The MCP fix is not portable to it without redesigning the LSP drain contract, and the LSP harness has no live failure. Recorded as a known latent risk only if an LSP test ever keeps two requests outstanding against a server that answers concurrently.

- timestamp: 2026-10-19
  checked: The REAL test, looped 30x per config from a snapshot of the GREEN test binary (scratchpad bin/contract_mcp_fix, the inbox_ rows pass from that copy; script scratchpad loop_fix.sh; TOKIO_WORKER_THREADS inherited by the spawned server). Nothing else building during the loops.
  found: default workers: 30/30 PASS (HEAD 7523963 baseline 4/30 FAIL). TOKIO_WORKER_THREADS=3: 30/30 PASS (baseline 10/30 FAIL).
  implication: The CI flake is gone under both configs. At the 3-worker baseline rate (1/3), 30 straight passes by luck has probability (2/3)^30, about 5e-6.

- timestamp: 2026-10-19
  checked: Whether the set-aside path is actually exercised by the real test, not just lucky ordering. Temporary `eprintln!("[diag] set aside id .. while waiting for ..")` in the set-aside branch. Built once, binary snapshotted to scratchpad bin/contract_mcp_diag, source restored from a byte backup (sha 50a786a, diff identical to the saved GREEN patch), rebuilt. The target-dir binary printed 0 diag lines afterwards. Looped 30x per config.
  found: default: 30/30 pass, 5 runs set aside a reply. TOKIO_WORKER_THREADS=3: 30/30 pass, 7 runs set aside a reply. Every set-aside run is the order that failed before the fix (32 before 31), and every one passed.
  implication: Direct observation that the fix handles the failing interleaving in the real binary. The set-aside rates (5/30, 7/30) match the HEAD failure rates (4/30, 10/30), so the remaining runs are the benign order.

- timestamp: 2026-10-19
  checked: Mutation check (guardrail signal 2). cargo-mutants not installed, so six manual mutants at the fix site (scratchpad mutants.sh, each applied alone to the GREEN file via perl, rebuilt, inbox_ rows run, file restored from the backup).
  found: M1 drop push_back (= the old drop behavior): KILLED by all 3 rows. M2 skip the set-aside lookup in expect_response: KILLED by inbox_keeps_an_out_of_order_reply_for_a_later_wait. M3 recv_next ignores set-aside: KILLED by inbox_recv_next_returns_set_aside_replies_first_in_arrival_order. M4 expect_silence ignores set-aside: KILLED by inbox_expect_silence_fails_while_a_reply_is_set_aside. M5 recv_next pops newest first: KILLED by the recv_next row. M6 push_front instead of push_back: KILLED by the recv_next row. 6/6 killed, mutation score 1.0. File restored to sha 50a786a; inbox_ 3/3 pass after.
  implication: Every behavioral line of the fix is pinned by at least one row, and each row pins a distinct line.

- timestamp: 2026-10-19
  checked: INCIDENT. Interference with the session manager's concurrent verification. While mutants.sh ran (about 09:57 onward), the session manager's gates_sm.sh (started 09:55) was running `cargo test --workspace --no-fail-fast` in the SAME shared target dir and on the SAME source file. Two toolchains are on this machine: /opt/homebrew/bin/cargo 1.94.1 (hash contract_mcp-6931a640...) and ~/.cargo/bin rustup 1.99.1 (hash contract_mcp-22cd0d5e...). mutants.sh puts ~/.cargo/bin first, and so does the session manager's run.
  found: scratchpad sm_test.log: `Running tests/contract_mcp.rs (.../contract_mcp-22cd0d5ea851dbec)` -> 14 passed, 1 failed (inbox_keeps_an_out_of_order_reply_for_a_later_wait, "no response with id 32 arrived within 1s" at contract_mcp.rs:305:68). In the GREEN file that panic is at line 316. Mutant M2 deletes exactly 11 lines from expect_response, which puts it at 305, and M2 is the only mutant that fails that row alone. mcp_tools_call_write_verbs_unknown passed in that run.
  implication: The session manager's contract_mcp result in that run came from MY M2 mutant binary, not the GREEN code. It is invalid and must be rerun. Its loop_green_sm.sh (09:51-09:54) finished before any mutant was applied. During that window the only source change was my one-line eprintln diag build (09:52, reverted at 09:52), so its 30/30 + 30/30 stands for GREEN logic. Its fmt and clippy (09:55-09:57) most likely saw the GREEN file but may have overlapped M1. My remaining builds use an isolated CARGO_TARGET_DIR and wait for gates_sm.sh to exit.

- timestamp: 2026-10-19
  checked: Revert-and-reconfirm (guardrail signal 5) and the CI gates, scratchpad verify_green.sh. Started after gates_sm.sh exited (10:26:56), in an ISOLATED CARGO_TARGET_DIR (scratchpad target-iso) with rustup cargo 1.99.1, so no shared artifact was touched. GREEN built first. Then the 38e3418 file (= GREEN minus the fix) was written, ONE test build was made and snapshotted (bin/contract_mcp_revert), and GREEN was restored at once (sha 50a786a verified) before anything ran.
  found: REVERT: inbox_ 0 passed / 3 failed; the real test at TOKIO_WORKER_THREADS=3 failed 7/20, every failure "no response with id 32 arrived within 15s" (the CI message). REAPPLY: inbox_ 3/3 pass; the real test at 3 workers 20/20 pass. Gates: `cargo fmt --all --check` exit 0; `cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo test --workspace --no-fail-fast` exit 0, 88 result lines, 2524 passed, 0 failed, 69 ignored (contract_mcp 15 passed, 1 ignored = the uv oracle; contract_lsp 3 passed); `cargo build -p ignition-cli --no-default-features` exit 0; `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` exit 0. File sha at the end 50a786a.
  implication: The bug returns when the fix is removed and goes away when it is put back, so this change is what fixes it. All CI gates pass, including a Windows-target lint of the test file.

- timestamp: 2026-10-19
  checked: Commit. `git add crates/ignition-cli/tests/contract_mcp.rs` only; `git commit` with no trailer.
  found: 8c424f1b5372f3a1a4e5757212c28f99981630d9 `fix(23): keep an MCP reply that arrives out of id order for its own wait`, 1 file, +42/-5. .planning/config.json, .planning/research/.cache/ and this debug file left uncommitted.
  implication: Fix landed as its own step on top of RED 38e3418. Not pushed.

## Eliminated

- hypothesis: (H1) the MCP server serializes tools/call or holds a lock across the slow gateway call, so unknown-tool replies queue behind the in-flight status call.
  evidence: mcp.rs:363-375 tokio::spawns every tools/call with no lock or semaphore; execute() returns -32601 for an unknown tool at the catalog lookup before any config/keyring/gateway work. Replay: 0/160 runs had 31 or 32 later than 24 ms while 30 took ~3 s. With TOKIO_WORKER_THREADS=1 (worst case for blocking) the real test passed 30/30.
  timestamp: 2026-10-19

- hypothesis: (H2) CI runner starvation (reader thread / process spawn) with a 15 s budget that is too tight.
  evidence: The response was not late. It arrived and was discarded by the harness (diag log, 8/8 failures). CI failures stamp at exactly 15.05-15.31 s after start with 31 already received, so the server was live. A larger budget cannot recover a consumed message.
  timestamp: 2026-10-19

## Resolution

root_cause: Test-harness defect in crates/ignition-cli/tests/contract_mcp.rs `ScriptedClient::expect_response` (lines 198-223 at 7523963; `Inbox::expect_response` after the 38e3418 extraction). It discards every received message whose id differs from the awaited id, although its own comment calls that case "legal out-of-order delivery". `mcp_tools_call_write_verbs_unknown` (line 1136) sends ids 31 and 32 back to back. The server spawns each tools/call as an independent task on a multi-thread runtime, so the two -32601 replies arrive in either order (about 25% 32-first with >=2 workers; 3 workers on macos-latest, 4 on ubuntu-latest). When 32 arrives first, expect_response(31) consumes and drops it, and expect_response(32) then times out after 15 s. The server is correct; the budget is not the problem.
fix: (RED 38e3418, GREEN 8c424f1) `Inbox` in crates/ignition-cli/tests/contract_mcp.rs keeps every reply it reads. A `set_aside: RefCell<VecDeque<Value>>` holds replies an id-matched wait read while looking for another id. expect_response claims a matching set-aside reply before reading the channel and pushes non-matching replies to the back instead of dropping them. recv_next returns set-aside replies first, oldest first, so arrival order holds. expect_silence fails while a set-aside reply is unclaimed. No server change, no budget change (RESPONSE_BUDGET_SECS and QUIET_WINDOW_MS untouched), the three RED rows unmodified, contract_lsp.rs unchanged (decision and evidence above: the LSP server answers requests in order, and the LSP data-plane test relies on stale publishes being discarded).
fix_commit: 8c424f1b5372f3a1a4e5757212c28f99981630d9
oracle_type: specified (the JSON-RPC contract: a reply is matched by id and never lost; the inbox_ rows assert exact ids and exact arrival order). Boundary neighbors covered by the rows: a reply set aside then claimed by the very next wait (32 then 30), several set aside in one wait including both id shapes ("b", 33), a set-aside reply never claimed (silence row).
verification: "fix 8c424f1 (RED 38e3418). inbox_ tests 3/3 (0/3 before). mcp_tools_call_write_verbs_unknown looped 30x default and 30x TOKIO_WORKER_THREADS=3: 30/30 pass each (HEAD 7523963: 4/30 and 10/30 fail). Revert: 0/3 and 7/20 fail; reapply 3/3 and 20/20. 6/6 hand mutants killed. Orchestrator rerun on the settled tree 2026-10-19: fmt 0, clippy -D warnings 0, cargo test --workspace --no-fail-fast 2524 passed 0 failed 69 ignored. An earlier gate run (sm_test.log) ran a mutant binary built concurrently in the shared target dir and is void. CI run on 8c424f1 pending."
  target_test:          { result: pass, detail: "inbox_ 3/3 (RED at 38e3418 was 0/3, confirmed independently by the session manager)" }
  mutation_check:       { result: pass, reason_if_skipped: "cargo-mutants not installed; six manual mutants at the fix site instead", mutant_killed: "6/6 (drop push_back, skip set-aside lookup, recv_next ignores set-aside, silence ignores set-aside, pop newest first, push_front)" }
  no_op_deletion:       { result: pass, deletion_justified_by_rca: n/a, detail: "+42/-5; the 5 removed lines are one struct literal and 4 doc/comment lines reworded; no branch, assertion or budget removed" }
  adjacent_tests:       { result: pass, suites_run: ["cargo test --workspace --no-fail-fast: 2524 passed, 0 failed, 69 ignored", "contract_mcp 15 passed + 1 ignored (uv oracle)", "contract_lsp 3 passed"] }
  revert_and_reconfirm: { result: pass, bug_returned_on_revert: true, fixed_on_reapply: true, detail: "revert: inbox_ 0/3 and real test at 3 workers 7/20 fail with the CI message; reapply: 3/3 and 20/20" }
  flake_loops:          { default_workers: "30/30 pass (HEAD baseline 4/30 fail)", tokio_worker_threads_3: "30/30 pass (baseline 10/30 fail)", instrumented: "60/60 pass, 12 of them hit the 32-before-31 order and set the reply aside (5 default, 7 at 3 workers)" }
  gates:                { fmt: "exit 0", clippy_workspace_all_targets_D_warnings: "exit 0", headless_build: "exit 0", xwin_clippy_windows_msvc: "exit 0", toolchain: "rustup cargo 1.99.1, isolated CARGO_TARGET_DIR" }
  not_verified:         "The Windows leg was not RUN (only cross-linted). GitHub Actions has not run 8c424f1 (no push, per constraints). The session manager's own gates_sm.sh contract_mcp result is invalid (it ran my M2 mutant binary) and needs a rerun."
  guardrail_verdict:    accepted
files_changed: [crates/ignition-cli/tests/contract_mcp.rs (RED 38e3418: Inbox extraction + 3 failing harness rows; GREEN 8c424f1: set-aside buffer in Inbox)]

## Prevention

five_whys (branching):
  - why did CI time out on id 32? The harness dropped 32's reply while waiting for 31.
  - why was it dropped? expect_response treated "a reply for another id" as noise to skip, not as data to keep. The comment said "legal out-of-order delivery - keep waiting" and the code kept waiting but threw the message away.
  - why did nobody notice before 23-14? Every earlier MCP test kept at most one request outstanding whose reply could be overtaken, except the ping-starvation test, which uses recv_next rather than expect_response. mcp_tools_call_write_verbs_unknown was the first test with two concurrent tools/call requests awaited in a fixed order.
  - why did it pass locally? The flip needs >=2 worker threads AND task 32 to finish first. The harness sends each frame as two writes, which gives 31 a head start, so on a 10-core Mac the real test failed only ~13% of the time. A full local run usually passed. CI runners (3-4 cores) fail more often.
  - branch, environment: CI core counts raise the flip rate. This is a trigger, not a defect.
why_not_caught: No gate tested the harness's own receive rules. The harness had no rows of its own, so its behavior on out-of-order replies was only ever exercised by chance through the server. Review accepted a comment that described the intended behavior rather than what the code did. Flakiness in CI was the first signal.
recurrence_guard: Three deterministic harness rows in crates/ignition-cli/tests/contract_mcp.rs, fed through a plain channel with no server and no scheduling: inbox_keeps_an_out_of_order_reply_for_a_later_wait, inbox_recv_next_returns_set_aside_replies_first_in_arrival_order, inbox_expect_silence_fails_while_a_reply_is_set_aside. Each pins a distinct line of the fix (6/6 mutants killed). Any future test with several outstanding requests is now safe in any reply order.
known_latent_risk: contract_lsp.rs LspClient::expect_response still drops non-matching messages by design (LSP notifications; the data-plane test depends on it). It is safe while the LSP server answers requests in order from one loop. If the LSP server ever handles requests concurrently, or a test keeps two LSP requests outstanding, apply the same set-aside idea there, but keep publishDiagnostics draining explicit.
kb_entry_draft: "mcp-write-verbs-timeout - MCP contract harness dropped an out-of-order JSON-RPC reply. Error patterns: no response with id N arrived within 15s, protocol death or starvation, flaky on CI only, always the second of two concurrent requests. Root cause: test harness expect_response discarded non-matching ids; server answers spawned tools/call tasks in either order. Fix: set-aside buffer in Inbox (8c424f1). Files: crates/ignition-cli/tests/contract_mcp.rs. Why not caught: harness receive rules had no tests of their own. Recurrence guard: inbox_ rows in contract_mcp.rs."

## Session notes

- Concurrency hazard for future sessions: two cargo toolchains exist on this machine (/opt/homebrew/bin/cargo 1.94.1 and ~/.cargo/bin rustup 1.99.1), and they build the same test into different hashes in the shared target dir. A mutation or revert experiment that edits a source file in the main checkout while another agent runs `cargo test` there will feed that agent mutant binaries. Run such experiments only in an isolated CARGO_TARGET_DIR, and only when no other cargo process is running against the checkout.
