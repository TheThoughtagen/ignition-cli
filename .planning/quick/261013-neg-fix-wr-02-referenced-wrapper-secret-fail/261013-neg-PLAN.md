---
phase: quick/261013-neg
plan: 01
type: execute
wave: 1
depends_on: []
autonomous: true
requirements: [QUICK-neg, DRND-07]
files_modified:
  - crates/ignition-core/src/deploy/lint.rs
  - crates/ignition-core/tests/deploy_render.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - README.md

estimate:
  tokens: 55000
  raw_tokens: 55000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "The verifier repro `{\"password\":{\"type\":\"Referenced\",\"data\":{\"providerName\":\"p\",\"secretName\":\"s\",\"password\":\"hunter2\"}}}` in `ignition/database-connection` config.json is refused as `embedded_secret` (exit 2), naming `<file>: password (plaintext)` exactly once (21-REVIEW WR-02)"
    - "An object is Referenced only when it has exactly the members `type` (the string `Referenced`) and `data`, and `data` has exactly the string members `providerName` and `secretName`, in either key order. Any other shape is refused as `(plaintext)` at a secret path: an extra, missing or wrong-case member, a non-string name, a non-object `data`, a sibling member beside `type`/`data`, or a `type` other than `Referenced`"
    - "No flag turns the key-name rule off. Inside a malformed Referenced subtree, a secret-looking member is refused by key name in an undescribed type (`settings.cred.data.password (plaintext, by key name)`) and warned about by path in a described type"
    - "The planted value never appears in error Display or Debug, in render warnings, or in CLI stdout or stderr, in both human and `--json` modes"
    - "A well-formed CD7 Referenced (`data{providerName, secretName}` then `type`, and the type-first order) still passes at schema paths and in undescribed types, with no warnings. deploy_corpus, deploy_render and contract_deploy (including RENDER_GOLDEN) stay green unchanged"
    - "README `#### Secrets` states the exact accepted Referenced shape, and says that any other shape is not a Referenced secret and is scanned as ordinary content"
  artifacts:
    - "crates/ignition-core/src/deploy/lint.rs: strict `is_referenced`. The Referenced exemption flag is gone from `Walker::walk`, and a well-formed Referenced is skipped as a whole. Unit tests and a proptest cover the repro, the variant matrix and the key-name subtree"
    - "crates/ignition-core/tests/deploy_render.rs: `CANARY_REFERENCED` plus render-level refusal and warning tests via `assert_no_canary`"
    - "crates/ignition-cli/tests/contract_deploy.rs: `CANARY_REFERENCED` plus CLI refusal and warning tests in human and `--json` modes"
    - "README.md: `#### Secrets` updated for the exact Referenced shape"
  key_links:
    - "`is_referenced` → `Walker::schema_value` (secret-path acceptance) and `Walker::walk` (whole-object skip). Both call sites now depend on the exact shape"
    - "`Walker::walk` key-name `hit` → `key_name_hit` → `SecretScan.refused` / `.warnings` → render's single `embedded_secret` raise → CLI stderr / `--json` envelope. Only locations travel; no value does"
    - "CD7 well-formed shape tests (`referenced_and_null_secrets_pass`, `empty_or_referenced_key_name_values_pass_in_undescribed_type`, `referenced_data_secret_name_is_a_name`) ↔ the strict check. They must stay green unedited"
---

<objective>
Close 21-REVIEW.md WR-02. The secret lint's Referenced check fails open. `is_referenced` accepts any object with `"type":"Referenced"` and an object `data`. The `in_ref_data` flag then turns the key-name rule off for the whole `data` subtree, and `schema_value` accepts the object at an OpenAPI secret-typed path. A plaintext secret can therefore hide inside a Referenced-looking wrapper, pass the lint, and be written by `deploy render --out`.

Purpose: restore DRND-07's fail-closed guarantee. No plaintext secret may pass the lint, and no value may reach any output.
Output: a strict `is_referenced` with no exemption flag; regression and canary tests at the unit, render and CLI layers; README `#### Secrets` brought into line.

Scope: WR-02 only. WR-01 (key-name rule for non-string values, wider needle list) and the other review items belong to separate tasks. Do not change `key_looks_secret`, the string-only key-hit behavior, or `SECRET_FIELDS`.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/phases/21-deploy-render/21-REVIEW.md
@.planning/phases/21-deploy-render/21-08-SUMMARY.md
@crates/ignition-core/src/deploy/lint.rs

Key facts, already confirmed during planning (do not re-derive):

- `Node::Obj` is `Vec<(String, Node)>` in source order (crates/ignition-core/src/deploy/node.rs:60-69). The parser refuses duplicate keys (node.rs:117), so "members.len() == 2 and both named members present" is an exact shape test. `Node::get(key)` finds a member by name.
- Today's fail-open path (lint.rs:46-51, 97-145): `schema_value` sees `is_referenced(value)` as true and walks the object. In `walk`, `skip = in_ref_data || (referenced && key == "data")` makes every member under `data` get `hit = false`. The repro therefore yields an empty `SecretScan`.
- The sibling variant `{"type":"Referenced","data":{"providerName":"p","secretName":"s"},"value":"x"}` at a secret path in a described type also passes today. It is the same fail-open. CD7 (21-02-SUMMARY) says the gateway writes exactly `data` then `type`.
- The sanitized corpus contains no `Referenced` object. Only tests and fixture scripts mention it. A stricter check therefore cannot move deploy_corpus output or `RENDER_GOLDEN` (crates/ignition-cli/tests/contract_deploy.rs:27).
- `com.cirruslink.mqtt.engine.gateway/server` has no secret paths and is not described by the OpenAPI, so key-name hits there are refused. `ignition/database-connection` is described, with secret path `password`, so key-name hits elsewhere there only warn.
- Existing helpers to reuse:
  - in lint.rs tests: `scan(module, ty, json)`, which uses origin `f/config.json`;
  - in deploy_render.rs: `secret_render(types, files)`, `assert_no_canary(&result, &[..])`, and the consts `DB`, `MQTT`, `MQTT_TYPE` (around line 1506);
  - in contract_deploy.rs: `secret_repo(extra_types, configs)`, `ign()`, `stderr_json(&out)` (around line 332), modeled on `embedded_secret_names_path_not_value` and `possible_secret_warning_names_path_not_value`.
- Project rule (CLAUDE.md): no git worktrees; build on the main checkout only (shared `CARGO_TARGET_DIR`). The contract tests run the cargo-built binary through `ign()`, so no `cargo install` is needed for the gates.
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1: Tracer: the verifier repro fails closed end to end (lint → render → CLI), value never echoed</name>
  <files>crates/ignition-core/src/deploy/lint.rs, crates/ignition-core/tests/deploy_render.rs, crates/ignition-cli/tests/contract_deploy.rs</files>
  <behavior>
    - lint unit `referenced_with_extra_data_member_at_schema_path_is_plaintext`: `scan("ignition","database-connection", <repro>)` gives `refused == ["f/config.json: password (plaintext)"]` and empty warnings. Today it gives an empty scan (RED).
    - lint unit `key_name_rule_applies_inside_malformed_referenced`: `scan("com.cirruslink.mqtt.engine.gateway","server", {"settings":{"cred":{"type":"Referenced","data":{"providerName":"p","secretName":"s","password":"hunter2"}}}})` gives `refused` equal to exactly `["f/config.json: settings.cred.data.secretName (plaintext, by key name)", "f/config.json: settings.cred.data.password (plaintext, by key name)"]`, in document order. A malformed wrapper's `data` is ordinary content, so its `secretName` is checked too. Today it gives an empty scan (RED).
    - render `malformed_referenced_secret_is_refused_by_path`: the repro with the value `CANARY-REFERENCED-WR02` at `DB` gives `err.code() == "embedded_secret"` and `exit_code() == 2`. The text contains `gateways/g/resources/ignition/database-connection/Main/config.json: password (plaintext)`, and `text.matches("password (").count() == 1`. The MQTT `settings.cred` variant at `MQTT` contains `Broker/config.json: settings.cred.data.password (plaintext, by key name)`. `assert_no_canary` passes for both.
    - CLI `malformed_referenced_secret_names_path_not_value`: both configs in one `secret_repo`, for json in [false, true]. Exit is 2 and stdout is empty. `CANARY-REFERENCED-WR02` is absent from stdout and stderr. The `--json` error code is `embedded_secret`. The human-mode stderr contains `embedded_secret` or `embedded or plaintext secret`. The message contains `ignition/database-connection/Main/config.json: password (plaintext)` and `server/Broker/config.json: settings.cred.data.password (plaintext, by key name)`.
  </behavior>
  <action>
RED first. Add the two lint unit tests above to the `#[cfg(test)] mod tests` in lint.rs. In deploy_render.rs, add `const CANARY_REFERENCED: &str = "CANARY-REFERENCED-WR02";` beside the other 21-08 canaries, plus the test `malformed_referenced_secret_is_refused_by_path`, built on `secret_render` and `assert_no_canary` like `embedded_secret_is_refused_by_path`. In contract_deploy.rs, add the same const beside the other canaries, plus the test `malformed_referenced_secret_names_path_not_value`, modeled on `embedded_secret_names_path_not_value`. Its types are `ignition/database-connection` and `com.cirruslink.mqtt.engine.gateway/server`. Its configs are `ignition/database-connection/Main/config.json` (the schema-path repro carrying the canary) and `com.cirruslink.mqtt.engine.gateway/server/Broker/config.json` (`{"settings":{"cred":{...repro data carrying the canary...}}}`). Run the new tests and confirm they fail, then commit as `test(quick-261013-neg): failing repro for Referenced wrapper fail-open (WR-02)`.

GREEN: in crates/ignition-core/src/deploy/lint.rs, make `is_referenced` exact. It returns true only when all of these hold:
1. The node is a `Node::Obj` with exactly two members, `type` and `data`.
2. `type` is `Node::Str("Referenced")`, compared exactly and case-sensitively.
3. `data` is a `Node::Obj` with exactly two members.
4. `data.get("providerName")` and `data.get("secretName")` are both `Node::Str`.

Key order does not matter (CD7 writes `data` then `type`, and tests also use type-first). Because the parser guarantees unique keys, the length checks plus named lookups are exact. No string content check is needed: these are a provider and a secret name, not values.

Requiring the wrapper itself to be exactly `{type, data}` is Claude's discretion, beyond the review's data-only fix. It closes the sibling-member variant of the same fail-open, matches CD7, and cannot affect the corpus, which holds no Referenced object.

Then remove the Referenced exemption mechanism from `Walker`. Drop the fourth parameter of `walk` and the `skip` computation, and compute the key-name `hit` from `rules.key_names && key_looks_secret(key)` alone. In the `Node::Obj` arm, right after the Embedded check, return early when `is_referenced(node)`. A well-formed Referenced holds only `type`, `providerName` and `secretName`: names, never a value, and no nested object that could be Embedded. Skipping it whole is equivalent to today's behavior for well-formed objects. It also makes it structurally impossible for any subtree to switch the key-name rule off. That is the requested guarantee for malformed subtrees: they are no longer Referenced, so they are walked as ordinary objects.

In `schema_value`, the Referenced arm becomes a no-op pass. Drop its walk call and the extra parameter, and update the `schema_value` call site in `walk`. Update the three `walk` call sites: the recursion, `scan_embedded` and `scan_secrets`.

Update the doc comments. The module doc (lines 4-8) should say "non-null, non-Referenced" with Referenced meaning the exact shape. The `is_referenced` doc should state the exact shape and CD7. The `walk` doc should drop the in-Referenced-data sentence. In the `scan_secrets` doc, "outside a Referenced object's `data`" becomes "outside a well-formed Referenced object". The doc should not mention any value.

Do not touch `key_looks_secret`, `key_name_hit`, `SecretScan`, the Embedded rule or the Windows/CR half. Commit as `fix(quick-261013-neg): strict Referenced shape, no key-name exemption (WR-02)`.
  </action>
  <verify>
    <automated>cargo test -p ignition-core --lib deploy::lint && cargo test -p ignition-core --test deploy_render referenced && cargo test -p ignition-cli --test contract_deploy malformed_referenced</automated>
  </verify>
  <done>Each layer refuses the verifier repro at the secret-typed path as `password (plaintext)`, and the CLI exits 2 with an empty stdout. The key-name rule fires inside a malformed Referenced subtree in an undescribed type. `CANARY-REFERENCED-WR02` appears in no Display, Debug, warning, stdout or stderr text in human or `--json` mode. The existing well-formed Referenced tests (`referenced_data_secret_name_is_a_name`, `schema_path_values`, `referenced_and_null_secrets_pass`, `empty_or_referenced_key_name_values_pass_in_undescribed_type`) pass unedited. Two commits: RED and GREEN.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Expand: malformed-shape matrix, both CD7 key orders, described-type warning path, value-free proptest</name>
  <files>crates/ignition-core/src/deploy/lint.rs, crates/ignition-core/tests/deploy_render.rs, crates/ignition-cli/tests/contract_deploy.rs</files>
  <behavior>
    - lint unit `malformed_referenced_variants_are_plaintext_at_schema_path`: each variant below, as the value of `password` in `ignition/database-connection`, gives exactly `["f/config.json: password (plaintext)"]` and no warnings. The assertion message carries the variant index, not the JSON text.
      - (a) `data` with an extra `password` member
      - (b) `data` without `secretName`
      - (c) `data` without `providerName`
      - (d) empty `data`
      - (e) `providerName` set to `1`
      - (f) `secretName` set to `null`
      - (g) `secretName` set to `{"v":"x"}`
      - (h) `data` set to `"x"`
      - (i) `data` set to `[]`
      - (j) a well-formed `data` plus a wrapper sibling `"value":"x"`
      - (k) `"type":"referenced"` in lowercase
      - (l) `"type":1`
      - (m) the `type` member missing
      - (n) a wrong-case key `providername`
    - lint unit `well_formed_referenced_passes_in_either_key_order`: `{"data":{"providerName":"p","secretName":"s"},"type":"Referenced"}` and `{"type":"Referenced","data":{"secretName":"s","providerName":"p"}}` both give `SecretScan::default()`. This holds as the database-connection `password`, and as MQTT `password` and `settings.clientSecret`.
    - lint unit `malformed_referenced_outside_schema_path_warns_in_described_type`: database-connection `{"settings":{"cred":<repro with value hunter2>}}` gives an empty `refused` and exactly 2 warnings. One warning contains `settings.cred.data.secretName;` and the other `settings.cred.data.password;`. Neither contains `hunter2`.
    - lint proptest `prop_extra_data_member_is_refused_without_value`: the generated key matches `[A-Za-z][A-Za-z0-9]{0,12}`, with `prop_assume!` excluding `providerName` and `secretName`. The value is `CANARY-` followed by `[A-Za-z0-9]{6,12}`. With the extra member added to a well-formed `data` at the database-connection `password`, `refused == ["f/config.json: password (plaintext)"]`. The value appears in no `refused` or `warnings` string.
    - render `malformed_referenced_in_described_type_warns_without_value`: the database-connection `{"settings":{"cred":<repro with CANARY_REFERENCED>}}` at `DB` renders Ok and passes `assert_no_canary`. It produces 2 warnings, one containing `config.json: settings.cred.data.password;`.
    - CLI `malformed_referenced_warning_names_path_not_value`: a `secret_repo` with that database-connection config, run for json in [false, true]. The exit code is 0, and `CANARY-REFERENCED-WR02` is absent from stdout and stderr. In `--json` mode, the stdout envelope's `data.warnings` has 2 entries, one containing `config.json: settings.cred.data.password;`. In human mode, stdout and stderr together contain `settings.cred.data.password`.
  </behavior>
  <action>
Add the tests above. Unit tests and the proptest go in lint.rs `mod tests`, which already imports `proptest::prelude::*` mid-module. Place the new proptest inside the existing `proptest!` block, or in a second `proptest!` block after it. The render test goes in deploy_render.rs next to Task 1's test, and the CLI test goes in contract_deploy.rs next to Task 1's test. Reuse `CANARY_REFERENCED` from Task 1.

These guard the strict shape from Task 1 against regressions: every way a Referenced-looking object can deviate from CD7, both legitimate key orders, and the described-type branch where key-name hits warn rather than refuse.

If any variant passes or any well-formed case is refused, fix `is_referenced` in lint.rs. Do not weaken the test. A described-type warning is the designed outcome for a non-schema path (21-08 decision: the schema is authoritative there). Do not convert it into a refusal.

Commit as `test(quick-261013-neg): Referenced shape matrix, key orders and warning canaries (WR-02)`.
  </action>
  <verify>
    <automated>cargo test -p ignition-core --lib deploy::lint && cargo test -p ignition-core --test deploy_render referenced && cargo test -p ignition-cli --test contract_deploy malformed_referenced</automated>
  </verify>
  <done>All 14 malformed variants are refused as `password (plaintext)` at the secret path. Both CD7 key orders pass with no warnings at schema and key-name positions. In a described type, the malformed subtree produces path-only warnings. The proptest shows that any extra `data` member is refused and its value never appears in findings. The render and CLI warning canaries pass in human and `--json` modes.</done>
</task>

<task type="auto">
  <name>Task 3: README `#### Secrets` states the exact Referenced shape; full deploy gates green</name>
  <files>README.md</files>
  <action>
In README.md `#### Secrets` (around line 2260), document the exact accepted shape after the Referenced example JSON block. A Referenced secret is an object with exactly two members: `type`, the string `"Referenced"`, and `data`. `data` holds exactly the two string members `providerName` and `secretName`. Member order does not matter; the gateway writes `data` then `type` (CD7).

Add a sentence containing the phrase `not a Referenced secret`. It should say that any other shape is not a Referenced secret: an extra member inside `data` or beside it, a missing member, or a non-string name. Such a value is refused as `(plaintext)` at a schema path. Elsewhere it is scanned like any other object, so the key-name rule (or its warning, in a described type) applies to its members.

In the "Plaintext at a schema path" bullet, say "absent, `null` or a well-formed Referenced". In the key-name bullet, change "The `secretName` of a Referenced object is a name" to "The `secretName` of a well-formed Referenced object is a name".

Leave the residual-risk paragraph, the type table and the Windows section unchanged. Keep the README's existing style: plain prose, decision IDs in parentheses, no values in examples beyond the placeholder names already used.

Then run the full deploy gates. If `cargo fmt --all --check` fails, run `cargo fmt --all` and re-check. Commit as `docs(quick-261013-neg): README Secrets states the exact Referenced shape (WR-02)`.
  </action>
  <verify>
    <automated>awk '/^#### Secrets/,/^#### Windows safety/' README.md | grep -c "not a Referenced secret" && awk '/^#### Secrets/,/^#### Windows safety/' README.md | grep -c "well-formed Referenced" && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ignition-core --test deploy_corpus && cargo test -p ignition-core --test deploy_render && cargo test -p ignition-cli --test contract_deploy</automated>
  </verify>
  <done>README `#### Secrets` names the exact Referenced shape and the `not a Referenced secret` rule, and says `well-formed Referenced` in both bullets. fmt, clippy (`-D warnings`), deploy_corpus, deploy_render and contract_deploy are all green, including `RENDER_GOLDEN` and `corpus_render_is_byte_identical`, with no edit to any golden or corpus expectation.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| declared repo tree → render lint | `gateways/<g>/resources/**/config.json` is author- or agent-supplied and untrusted. The lint is the only gate before `deploy render --out` writes the tree and before Phase 22/23 ship it |
| lint findings → human and agent outputs | Error Display/Debug, render warnings, CLI stderr and the `--json` envelope are read by humans, agents and logs, and must carry locations only |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-neg-01 | Information disclosure | `lint.rs` `is_referenced` / `schema_value` | high | mitigate | Exact shape: the wrapper is exactly `{type, data}` and `data` is exactly the string members `{providerName, secretName}`. Anything else at a secret path is `(plaintext)`. Covered by the Task 1 repro and the Task 2 14-variant matrix and proptest |
| T-neg-02 | Information disclosure | `lint.rs` `Walker::walk` key-name exemption | high | mitigate | The Referenced exemption flag is removed. Only an exact-shape Referenced is skipped, and it holds names only, so a malformed subtree is walked with the key-name rule live. Covered by `key_name_rule_applies_inside_malformed_referenced` and the described-type warning test |
| T-neg-03 | Information disclosure | findings, warnings, CLI streams, `--json` | high | mitigate | `SecretScan` stays location-only. `CANARY-REFERENCED-WR02` is asserted absent from Display, Debug, warnings, stdout and stderr in human and `--json` modes (render and CLI tests), and the proptest asserts a generated value never appears in findings |
| T-neg-04 | Denial of service | legitimate CD7 Referenced refused (false positive) | low | mitigate | Both key orders are tested at schema and key-name positions. Existing well-formed tests stay unedited. The corpus holds no Referenced, so deploy_corpus and `RENDER_GOLDEN` are unchanged |

No package installs; no supply-chain row.
</threat_model>

<verification>
- `cargo test -p ignition-core --lib deploy::lint`: new unit tests and the proptest pass, alongside all 21-08 lint tests.
- `cargo test -p ignition-core --test deploy_render` and `--test deploy_corpus`: green, with no golden or corpus expectation edited.
- `cargo test -p ignition-cli --test contract_deploy`: green, including `RENDER_GOLDEN` and both new canary tests.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- Optional final sweep, run in the background: `cargo test --workspace` (21-08 ran it clean).
- `git diff --stat` touches only lint.rs, deploy_render.rs, contract_deploy.rs and README.md.
</verification>

<success_criteria>
- The verifier repro at a secret-typed path is refused as `embedded_secret` (exit 2), named by path as `password (plaintext)`, with the value never echoed.
- Only an exact CD7-shaped Referenced is accepted. Every malformed shape is refused at secret paths and scanned as ordinary content elsewhere.
- The key-name rule is never disabled for any subtree.
- Canary absence is proven across Display, Debug, warnings and CLI stdout and stderr, in both modes.
- The CD7 well-formed Referenced still passes, and deploy_corpus, deploy_render, contract_deploy and RENDER_GOLDEN are green.
- README `#### Secrets` matches the new behavior.
</success_criteria>

<output>
Create `.planning/quick/261013-neg-fix-wr-02-referenced-wrapper-secret-fail/261013-neg-SUMMARY.md` when done.
</output>
