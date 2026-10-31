---
phase: 261031-bbs-deploy-read-back-judges-only-what-the-ap
reviewed: 2026-10-31T16:08:45Z
depth: quick
files_reviewed: 12
files_reviewed_list:
  - README.md
  - crates/ignition-cli/src/render.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/deploy/apply/converge.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/pair.rs
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/tests/common/fake_deploy.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-core/tests/live_deploy_apply.rs
findings:
  critical: 1
  warning: 4
  info: 7
  total: 12
status: issues_found
---

# Quick 261031-bbs: Code Review Report

**Reviewed:** 2026-10-31T16:08:45Z
**Depth:** quick (requested). The read_back judge, the baseline build, the apply wiring and the README were traced by hand, because the request centres on false greens.
**Files Reviewed:** 12
**Status:** issues_found

## Summary

Scope: `git diff 208c6bf..HEAD -- . ':!.planning'` on `fix/deploy-read-back-scope`.

Most of the design holds up when traced:

- Plan keys and `Expected` keys are both `module/type/name`, so the `written` map lines up with the resources.
- An `added` unit is judged strictly.
- A `changed` unit that is missing afterwards is red.
- A `changed` unit with no live comparison, or with a `missing` or `system` pre-image, falls back to strict.
- The standalone judge only softens a resource whose plan status is exactly `same`. A resource with no status stays red.
- The baseline reads happen after the `.gwbk` and before `prepare`, outside the lock. A failed read propagates with `?` before anything is staged.
- `converged()` treats `warn` as not-fail, so the backup can converge with warn findings.
- `Debug` for `Remote` and `Compare` withholds values.
- The live test is gated by `IGNITION_LIVE_DEPLOY=1`, a loopback-URL guard and a gateway-name guard.

One false-green path remains, and the README misdescribes it as closed. Amendment A1 ("the write did not take effect") is judged on the whole REST view of a resource. Ancestor and `(root)` coverage then hides every written path under a subtree that already differed. Together they let a **partial** write pass verify with no rollback. The production resources this task exists for are exactly the ones exposed: OPC UA devices differ at `settings.deviceSettings` as a whole, Cirrus Link namespace-server-sets differ at `(root)`, and gateway-network-settings differs at `whitelist`.

## Critical Issues

### CR-01: A partial write to a covered path passes post-apply verify (A1 only looks at the whole resource), and the README says this case is caught

**File:** `crates/ignition-core/src/deploy/verify.rs:1106-1130` (the `LiveFile` branch of `judge_resource`), `crates/ignition-core/src/deploy/verify.rs:751-760` (`path_covered`), `README.md:3601-3606` and `README.md:3614-3617`

**Issue:** For a written `changed` unit, a post-apply difference is excused when:

1. every differing path is covered by the pre-apply set. It is covered when it, an ancestor of it, or `(root)` already differed from the live file. And
2. `!(write_differs && rest_moved(r0, r1).is_empty())`. In words: **any** path anywhere in the resource's REST view moved.

The A1 test runs on the whole resource, not on each written path. So when a write changes two members and the gateway applies one but ignores the other, rule 2 is satisfied as long as the ignored member is covered. The result is a `warn`, a green verdict, no rollback, and the wrong config stays live in production.

Ancestor coverage makes the hidden area much larger than "a path the gateway normalises":
- `diff_exact` emits the **parent** path when a key is absent on one side, when the types differ, or when arrays differ in length (`verify.rs` `diff_exact`, the `None => out.push(child_path(..))` arm and the `root_or(path)` arms).
- So a single pre-apply difference like `settings.deviceSettings` (production: Chiller1/2, Viewmarq_Barreling) covers every PLC address, port and timeout under it.
- A `(root)` difference (production: Cirrus `namespace-server-set/1,2,7,8`) covers the whole resource.

Concrete trace, `com.inductiveautomation.opcua/device/Chiller1`:
- Before: the REST view differs from the live file at `settings.deviceSettings`.
- The repo changes `settings.deviceSettings.hostname` and `settings.enabled`.
- The gateway applies `enabled` and ignores the hostname.
- After: `p = {settings.deviceSettings.hostname}`, which `settings.deviceSettings` covers, so `new` is empty.
- `write_differs` is true, but `rest_moved = {settings.enabled}` is not empty, so A1 does not fire. The verdict is warn, and the apply reports `applied`.

The README is wrong about this:
- `README.md:3616` says "(the A1 rule above still catches a write that did not take effect)".
- `README.md:3605` says A1 "also closes a difference covered at the root".

Both hold only when **nothing** in the resource moved. The code's doc comment (`verify.rs` above `read_back_check_with`) states the narrower truth ("unless the whole REST view stood still"). The README, which the operator reads, does not.

**Fix:** Make A1 per written path. Compute the write's intent once in `ReadBackBaseline::build` and store it on the entry. Then require each covered intended path to show REST movement on that path, an ancestor or a descendant:

```rust
// in build(), for the LiveFile basis:
let mut intent = Vec::new();
diff_exact(&strip_nulls(desired_value), &strip_nulls(live_value), "", &mut intent);
// store `intent: BTreeSet<String>` on BaselineEntry instead of `write_differs: bool`

// in judge_resource, LiveFile branch, after the `new` check:
let moved = rest_moved(r0, r1);
let overlaps = |a: &str, b: &str| a == b
    || path_covered(a, &BTreeSet::from([b.to_string()]))
    || path_covered(b, &BTreeSet::from([a.to_string()]));
let stuck: BTreeSet<String> = entry.intent.iter()
    .filter(|w| p.iter().any(|q| overlaps(q, w)))          // still differs there
    .filter(|w| !moved.iter().any(|m| overlaps(m, w)))     // and REST did not move there
    .cloned().collect();
if !stuck.is_empty() {
    return Verdict::Red(finding("resource_differs", key,
        format!("differs at: {} (the REST view did not change there during this apply, so that part of the write did not take effect)", join_set(&stuck))));
}
```

For the `Attributes` and `NameKeyedSubset` compare kinds, use the paths those comparisons produce instead of `diff_exact`. Add tests:
- `rb_judge_partial_write_covered_path_fails`: two written members, one applied and one ignored under a covered ancestor.
- The same case under `(root)` coverage.

Until the fix lands, correct `README.md:3605` and `README.md:3616` so they say A1 fires only when the resource's whole REST view stood still.

## Warnings

### WR-01: A written unit whose post-apply effective copy is in the `system` collection is never judged (false green)

**File:** `crates/ignition-core/src/deploy/verify.rs:944-951`

**Issue:** `read_back_check_with` hits `Remote::System` before it consults the judge. It pushes a warnings line and `continue`s, so no finding is made and the resource is not counted, whatever the baseline says. Take a written `changed` unit whose pre-apply view was `Config` (file-backed) and whose post-apply view is `System`. Its file-backed copy has dropped out of the effective view, which means the gateway did not take the written file. Verify still passes. The strict judge had the same gap. This change now records `before` on each `BaselineEntry`, so the evidence is at hand and unused.

**Fix:** Route `System` through `judge_resource`. In the `Baseline` arm, return `Red` when `written.is_some()`, or when `before` is `Config` and `after` is `System`. Keep the warning only for an unwritten resource that was `System` before as well. Add a judge test for `(LiveFile, Config) -> System`.

### WR-02: Deleted (`removed`) units are not verified after the apply, and the README says `files` judges them

**File:** `README.md:3610-3611`, `crates/ignition-core/src/deploy/verify.rs:1354-1356`

**Issue:** CONTEXT.md requires "A deleted unit: the REST view must show it absent (as today)". Nothing in the post-apply verify checks this:
- `Expected::from_render` only covers rendered resources.
- On the master, `check_files` iterates `expected.tag_units` only. Deleted non-tag units are not files-checked after the commit.
- So a historian provider or OPC device that the run deleted, but that the gateway still serves over REST (because the scan did not drop it, or a `core` copy took over), produces no red.

The README line "`files`, the plan and the ledger judge them" is inaccurate for the master. Only the host commit's own move and delete accounting judges them. On the backup, `files` does compare deleted paths.

**Fix:** At minimum, correct the README. Better: add the plan's `Removed` non-tag keys to the `read_back` sweep as expected-absent. Red when the post-apply REST effective copy is file-backed (`Config` in the target's mode collection). Pass when it is `Missing`, or when the effective copy is `core` or `system` and the plan expected that.

### WR-03: Standalone `deploy verify` (and `--node backup|both`) is now green when the gateway is not running the repo's config but the files are `same`

**File:** `crates/ignition-core/src/deploy/verify.rs:1052-1062`, `crates/ignition-core/src/actions/deploy.rs:1463-1467`, `README.md:3356` and `README.md:3396-3408`

**Issue:** R2 turns every REST difference, and every missing resource, on a `same` resource into a warn. "The files on disk equal the repo" does not prove "the gateway loaded them". Three cases go green that were red before:
- files copied in without a config scan;
- a scan that ignored a file without logging `will not be loaded`;
- a backup whose files synced but whose gateway never reloaded.

The `scan` check only catches a scan that is running now, and `loaded` only catches the one log phrase. The README tells operators the difference comes "through filled defaults, normalisation, or module-specific shapes". The code cannot know the cause. The user chose "warn, not red", so this is a documentation and observability gap, not a request to revert.

**Fix:**
- Reword the README so it says verify green no longer proves the REST effective view matches the repo for `same` resources, and that the warning names a difference of unknown cause.
- Consider an opt-in `--strict-read-back` for operators who want the old contract, or keep the resource red when the last scan predates the unit's live mtime.
- The README's "on a `changed` or `added` resource it stays red" (`README.md:3356`) should also say "or one with no plan status".

### WR-04: A `changed` unit that is missing from (or `system` in) the REST view before the apply can never be applied

**File:** `crates/ignition-core/src/deploy/verify.rs:623`, `crates/ignition-core/src/deploy/verify.rs:1066-1076`

**Issue:** `(Some(_), _) => BaselineBasis::Strict`, so a written unit whose pre-apply view is `Missing` is judged strictly. Missing afterwards is then red. Production already has one such resource: `ignition/edge-sync-settings` is "not in the gateway's effective view" while its files are `same`. Any future change to it, or to any resource the gateway never surfaces over REST, rolls back on every apply. This is a false red, not a false green, but it blocks the production use this task is for. Neither the README nor the residual-risk list mentions it.

**Fix:** Missing before and missing after on a written `changed` unit means REST cannot observe the resource. Downgrade it to a warn naming that reason, and rely on `files`, `loaded` and the commit for that unit. Alternatively, document it as a known blocker in the README's read-back baseline section. Add a judge test for `Written::Changed(_)` with `Missing -> Missing`.

## Info

### IN-01: `rest_sha256` is a whole-config hash of values, which works as a guessing oracle for low-entropy plaintext fields

**File:** `crates/ignition-core/src/deploy/verify.rs:662-671`, `crates/ignition-core/src/deploy/verify.rs:713-745`

**Issue:** `read_back_baseline.json` holds no plaintext values, and there is no per-path hash (R5 holds). But the file has an unsalted SHA-256 of the full REST `config`, and the declared `config.json` in the repo gives the rest of the document. Anyone with the run directory and the repo can test guesses for a single unknown plaintext field (some module REST views return passwords or keys in clear) offline. CONTEXT allowed hashes, so this is noted rather than raised. The README's "never a value" claim is literally true.

**Fix:** Hash with a per-run random salt that is not kept, or with an HMAC keyed by a value thrown away after the run. That keeps "same view within this run" without an oracle. Or drop `rest_sha256`: the judge never reads it.

### IN-02: The canonical hash is not number-canonical, so it disagrees with the comparison rules

**File:** `crates/ignition-core/src/deploy/verify.rs:740`

**Issue:** `scalar.to_string()` writes `1` and `1.0` differently, but `same_scalar` treats them as equal. Nulls inside arrays are kept, because `strip_nulls` only removes object members. Two REST views that `read_back` treats as identical can therefore get different `rest_sha256`s. That is harmless for a record. It is misleading if anyone diffs hashes across runs to ask "did the view change?".

**Fix:** Say in the doc comment that the hash is byte-canonical, not semantic. Or normalise integral floats to integers before hashing.

### IN-03: Paths with `.` or `[` in keys, and keys truncated by `printable`, can collide in `path_covered`

**File:** `crates/ignition-core/src/deploy/verify.rs:751-760`, and `printable` and `child_path` in the same file

**Issue:** Coverage now runs on the joined path strings. A literal key `a.b` is "covered" by a pre-apply difference at `a`. Two keys longer than 96 characters that share a prefix both become `<prefix>...`. Either way, a new path can be excused by an unrelated sibling. This is unlikely with Ignition configs, but it is a correctness gap in a safety check.

**Fix:** Build the sets from `Vec<String>` segments rather than the joined string. Or escape `.` and `[` in keys inside `child_path` before coverage is computed. Keep `printable` for display only.

### IN-04: `converge::Baseline::read_back` is an `Option` whose `None` branch cannot be reached

**File:** `crates/ignition-core/src/deploy/apply/converge.rs:58`, `crates/ignition-core/src/deploy/apply/mod.rs:1541`

**Issue:** `read_back_baselines` returns `Some` whenever `ctx.backup` is `Some`, and `converge_backup` only runs then. The strict fallback is safe: it can only produce a false red, which leaves the backup unsynced and does not undo the master. Still, the `Option` hides that invariant.

**Fix:** Return `Option<(ReadBackBaseline /*master*/, ...)>` paired with the backup handle, or pass a plain `ReadBackBaseline` into `converge_backup`.

### IN-05: `pair::read_back_backup_check` (the strict wrapper) is now used only by tests

**File:** `crates/ignition-core/src/deploy/pair.rs:469-476`

**Fix:** Remove it and point `pair/tests.rs` at `read_back_backup_check_with(.., &ReadBackJudge::Strict, ..)`, or mark it `#[cfg(test)]`.

### IN-06: The live test's cleanup docs promise more than the guard does, and the leftover check ignores the seeded alarm unit

**File:** `crates/ignition-core/tests/live_deploy_apply.rs:2142-2143`, `live_deploy_apply.rs:291-295`, `live_deploy_apply.rs:1349-1363`

**Issue:**
- The doc says cleanup "scans, even on a failure". `Clean::drop` only sweeps files, so after a panic the gateway keeps the seeded alarm config loaded until the next test's `forget_units` scan.
- `forget_units` returns `translators_in_mode` only, so `leftovers.is_empty()` never checks that `ignition/general-alarm-settings` left the gateway.
- The guards themselves are sound: env opt-in, loopback URL and fixture gateway name.
- There is no live negative case: an ignored or partial write that must roll back.

**Fix:**
- Fix the doc comment.
- Check the alarm singleton's `collection` is no longer the mode collection after `forget_units`.
- Consider a live negative test once CR-01 is fixed.

### IN-07: Smaller README gaps

**File:** `README.md:3436-3442`, `README.md:3583-3588`

**Issue:**
- The backup verify text names only "a backup with no file transport under `--node backup`" as the strict fallback. The code (`actions/deploy.rs:1463`) also falls back to strict when the backup or master files read fails.
- The baseline section says a failed baseline read "ends the run" but does not give the slug or exit code. It is a raw `CoreError`, not `deploy_preflight_failed`, so agents cannot match on it.

**Fix:** Name both strict-fallback cases, and state the error envelope a failed baseline read produces.

---

_Reviewed: 2026-10-31T16:08:45Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: quick (hand-traced for the false-green focus)_
