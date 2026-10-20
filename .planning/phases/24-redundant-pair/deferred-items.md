# Phase 24 deferred items

## From 24-04

- `crates/ignition-core/tests/live_gateway.rs::guard_drop_during_unwind_inside_runtime_still_cleans_up` failed in two full `cargo test --workspace --no-fail-fast` runs ("cleanup find + delete must hit the mock after a mid-runtime unwind drop", line 1069) and passed when run alone (`cargo test -p ignition-core --test live_gateway`). It asserts that a best-effort Drop-path cleanup reached a wiremock server, which is timing-sensitive under the load of the full workspace run. It does not touch redundancy code and 24-04 did not modify `ignition-core` beyond `actions/redundancy.rs`. Out of scope for 24-04; not fixed.

## From 24-05

- The same `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` failed in the 24-05 full workspace run and in three single reruns (13 to 27 s each) with the machine load average at 36 to 44. Same assertion (line 1069: the Drop-path cleanup find and delete did not reach the mock within 10 s). No 24-05 change is on its path. Not fixed; still timing-sensitive under heavy load.

