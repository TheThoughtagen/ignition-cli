//! Pure gate evaluation and refusal prose (DAPL-01, DAPL-02).
//!
//! [`evaluate_gates`] decides, from a plan and the caller's flags alone,
//! whether an apply may proceed. It reads no host, no gateway and no clock.

use crate::actions::deploy::DeployPlanResult;
use crate::deploy::plan::{CoreRelation, PlanEntry, PlanStatus};
use crate::error::{APPLY_LIST_SHOWN, CoreError};

/// The longest accepted `--allow-drift` reason, in characters.
pub const DRIFT_REASON_MAX_CHARS: usize = 500;

/// The caller's gate flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GateFlags {
    /// `--plan-sha256`: the hash the reviewer saw.
    pub plan_sha256: Option<String>,
    /// `--allow-destructive`.
    pub allow_destructive: bool,
    /// `--allow-drift <reason>`.
    pub allow_drift: Option<String>,
}

/// What the gates decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    /// The plan is empty: nothing to write.
    Noop,
    /// Every gate passed; `prose` is the plan as the confirmation text.
    Ready {
        /// The refusal and confirmation text, byte-stable for a plan.
        prose: String,
    },
}

/// Evaluate the gates in the confirmed order.
///
/// 1. A malformed flag is `invalid_input` (exit 2), whatever the plan says.
/// 2. A `--plan-sha256` that differs from the recomputed `plan_sha256` is
///    `plan_stale` (exit 6), so a flag the reviewer gave for an old plan can
///    never satisfy a gate of a new one.
/// 3. A plan with nothing to add, change or remove is [`GateOutcome::Noop`].
/// 4. Any entry whose `core` copy shadows its removal is `deploy_blocked`
///    (exit 6), whatever the flags.
/// 5. Destructive entries without `--allow-destructive`, then drifted
///    entries without `--allow-drift`, are `destructive_change_refused`
///    (exit 2).
/// 6. Otherwise [`GateOutcome::Ready`].
///
/// `unmanaged` entries never gate, and drift is judged only where
/// `drift == Some(true)`, which exists only with `--base`.
pub fn evaluate_gates(
    plan: &DeployPlanResult,
    flags: &GateFlags,
) -> Result<GateOutcome, CoreError> {
    if let Some(hash) = &flags.plan_sha256 {
        validate_plan_sha256(hash)?;
    }
    if let Some(reason) = &flags.allow_drift {
        validate_drift_reason(reason)?;
    }
    if let Some(expected) = &flags.plan_sha256
        && *expected != plan.plan_sha256
    {
        return Err(CoreError::PlanStale {
            expected: expected.clone(),
            actual: plan.plan_sha256.clone(),
            detail: "review hash".to_string(),
        });
    }
    if !plan.summary.has_changes {
        return Ok(GateOutcome::Noop);
    }

    let gated = || {
        plan.entries
            .iter()
            .filter(|e| e.status != PlanStatus::Unmanaged)
    };
    let blocked = sorted_keys(gated().filter(|e| e.core == CoreRelation::ShadowsRemoval));
    if !blocked.is_empty() {
        return Err(CoreError::DeployBlocked { keys: blocked });
    }
    if !flags.allow_destructive {
        let keys = sorted_keys(gated().filter(|e| e.destructive));
        if !keys.is_empty() {
            return Err(refusal("destructive", keys));
        }
    }
    if flags.allow_drift.is_none() {
        let keys = sorted_keys(gated().filter(|e| e.drift == Some(true)));
        if !keys.is_empty() {
            return Err(refusal("drift", keys));
        }
    }
    Ok(GateOutcome::Ready {
        prose: confirmation_prose(plan),
    })
}

/// The entry keys, byte-sorted.
fn sorted_keys<'a>(entries: impl Iterator<Item = &'a PlanEntry>) -> Vec<String> {
    let mut keys: Vec<String> = entries.map(|e| e.key.clone()).collect();
    keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    keys
}

/// A `destructive_change_refused` carrying the first
/// [`APPLY_LIST_SHOWN`] keys and the full count.
fn refusal(gate: &str, mut keys: Vec<String>) -> CoreError {
    let count = keys.len();
    keys.truncate(APPLY_LIST_SHOWN);
    CoreError::DestructiveChangeRefused {
        gate: gate.to_string(),
        keys,
        count,
    }
}

/// The plan as the refusal text: the target, the counts, one line per
/// changing entry (key, status, flags), the delete-set size and the hash.
/// Never file content and never a diff; byte-stable for a plan.
pub fn confirmation_prose(plan: &DeployPlanResult) -> String {
    use std::fmt::Write as _;

    let s = &plan.summary;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "deploy apply: env {}, gateway {}, mode {}",
        plan.env, plan.gateway, plan.mode
    );
    let _ = writeln!(
        out,
        "plan: {} added, {} changed, {} removed, {} same, {} unmanaged",
        s.added, s.changed, s.removed, s.same, s.unmanaged
    );
    for entry in &plan.entries {
        if matches!(entry.status, PlanStatus::Same | PlanStatus::Unmanaged) {
            continue;
        }
        let mut flags: Vec<&str> = Vec::new();
        if entry.destructive {
            flags.push("destructive");
        }
        if entry.lockout_risk {
            flags.push("lockout_risk");
        }
        if entry.drift == Some(true) {
            flags.push("drift");
        }
        if !matches!(entry.core, CoreRelation::None | CoreRelation::Identical) {
            flags.push(entry.core.word());
        }
        if flags.is_empty() {
            let _ = writeln!(out, "  {} {}", entry.status.word(), entry.key);
        } else {
            let _ = writeln!(
                out,
                "  {} {} [{}]",
                entry.status.word(),
                entry.key,
                flags.join(", ")
            );
        }
    }
    let _ = writeln!(out, "delete set: {} file(s)", plan.delete_set.files.len());
    let _ = writeln!(out, "plan_sha256: {}", plan.plan_sha256);
    out.push_str(
        "rerun with --yes to apply this plan; when not interactive (--json, or stdin or \
         stdout not a terminal) also pass --plan-sha256 with the hash above\n",
    );
    out
}

/// `--allow-drift` needs a non-empty reason of at most
/// [`DRIFT_REASON_MAX_CHARS`] characters and no control characters. The
/// reason is recorded in run records and logs, so nothing else may ride it.
pub fn validate_drift_reason(reason: &str) -> Result<(), CoreError> {
    let problem = if reason.is_empty() {
        Some("must not be empty".to_string())
    } else if reason.chars().count() > DRIFT_REASON_MAX_CHARS {
        Some(format!(
            "must be at most {DRIFT_REASON_MAX_CHARS} characters"
        ))
    } else if reason.chars().any(char::is_control) {
        Some("must not contain control characters".to_string())
    } else {
        None
    };
    match problem {
        None => Ok(()),
        Some(problem) => Err(CoreError::InvalidInput {
            reason: format!("--allow-drift reason {problem}"),
        }),
    }
}

/// `--plan-sha256` must be 64 lowercase hex characters.
pub fn validate_plan_sha256(value: &str) -> Result<(), CoreError> {
    let ok = value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if ok {
        Ok(())
    } else {
        Err(CoreError::InvalidInput {
            reason: "--plan-sha256 must be 64 lowercase hex characters (the plan_sha256 \
                     of the reviewed plan)"
                .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DRIFT_REASON_MAX_CHARS, GateFlags, GateOutcome, confirmation_prose, evaluate_gates,
        validate_drift_reason, validate_plan_sha256,
    };
    use crate::actions::deploy::DeployPlanResult;
    use crate::deploy::plan::{CoreRelation, PlanEntry, PlanStatus, PlanSummary};
    use crate::deploy::transport::DeleteSet;

    const HASH: &str = "ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12";

    fn entry(key: &str, status: PlanStatus) -> PlanEntry {
        PlanEntry {
            key: key.to_string(),
            module: "ignition".to_string(),
            ty: "database-translator".to_string(),
            name: None,
            status,
            live_in: Vec::new(),
            core: CoreRelation::None,
            destructive: false,
            destructive_reason: None,
            lockout_risk: false,
            drift: None,
            files: Vec::new(),
            diff: None,
        }
    }

    fn plan_of(entries: Vec<PlanEntry>) -> DeployPlanResult {
        let mut summary = PlanSummary::default();
        for e in &entries {
            match e.status {
                PlanStatus::Added => summary.added += 1,
                PlanStatus::Changed => summary.changed += 1,
                PlanStatus::Removed => summary.removed += 1,
                PlanStatus::Same => summary.same += 1,
                PlanStatus::Unmanaged => summary.unmanaged += 1,
            }
        }
        summary.has_changes = summary.added + summary.changed + summary.removed > 0;
        DeployPlanResult {
            env: "prod".to_string(),
            gateway: "gw".to_string(),
            mode: "production".to_string(),
            node: "master".to_string(),
            transport: "local".to_string(),
            render_sha256: "0".repeat(64),
            plan_sha256: HASH.to_string(),
            summary,
            entries,
            delete_set: DeleteSet::default(),
            base: None,
            warnings: Vec::new(),
        }
    }

    fn destructive(key: &str) -> PlanEntry {
        let mut e = entry(key, PlanStatus::Removed);
        e.destructive = true;
        e.destructive_reason = Some("removes a resource".to_string());
        e
    }

    fn drifted(key: &str) -> PlanEntry {
        let mut e = entry(key, PlanStatus::Changed);
        e.drift = Some(true);
        e
    }

    fn flags(destructive: bool, drift: Option<&str>) -> GateFlags {
        GateFlags {
            plan_sha256: None,
            allow_destructive: destructive,
            allow_drift: drift.map(str::to_string),
        }
    }

    fn refusal(plan: &DeployPlanResult, flags: &GateFlags) -> (String, u8, String) {
        let err = evaluate_gates(plan, flags).expect_err("the gates refuse");
        (err.code().to_string(), err.exit_code(), err.to_string())
    }

    #[test]
    fn plan_sha256_accepts_64_lowercase_hex() {
        assert!(validate_plan_sha256(&"a1".repeat(32)).is_ok());
        assert!(validate_plan_sha256(&"0".repeat(64)).is_ok());
    }

    #[test]
    fn plan_sha256_refuses_short_uppercase_and_non_hex() {
        for bad in [
            "a".repeat(63),
            "A1".repeat(32),
            "g".repeat(64),
            String::new(),
            format!("{} ", "a".repeat(63)),
        ] {
            let err = validate_plan_sha256(&bad).expect_err(&bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
            assert_eq!(err.exit_code(), 2);
        }
    }

    #[test]
    fn order_stale_beats_noop() {
        let plan = plan_of(vec![entry("ignition/a/x", PlanStatus::Same)]);
        let mut f = flags(false, None);
        f.plan_sha256 = Some("1".repeat(64));
        let (code, exit, _) = refusal(&plan, &f);
        assert_eq!((code.as_str(), exit), ("plan_stale", 6));
    }

    #[test]
    fn order_stale_beats_the_destructive_gate() {
        let plan = plan_of(vec![destructive("ignition/a/x")]);
        let mut f = flags(false, None);
        f.plan_sha256 = Some("1".repeat(64));
        assert_eq!(refusal(&plan, &f).0, "plan_stale");
    }

    #[test]
    fn order_empty_plan_is_noop() {
        let plan = plan_of(vec![
            entry("ignition/a/x", PlanStatus::Same),
            entry("ignition/a/y", PlanStatus::Unmanaged),
        ]);
        assert_eq!(
            evaluate_gates(&plan, &flags(false, None)).expect("noop"),
            GateOutcome::Noop
        );
        let mut matching = flags(false, None);
        matching.plan_sha256 = Some(HASH.to_string());
        assert_eq!(
            evaluate_gates(&plan, &matching).expect("noop with its own hash"),
            GateOutcome::Noop
        );
    }

    #[test]
    fn order_shadows_removal_blocks_even_with_both_flags() {
        let mut shadowed = destructive("ignition/a/x");
        shadowed.core = CoreRelation::ShadowsRemoval;
        let plan = plan_of(vec![shadowed, entry("ignition/a/y", PlanStatus::Added)]);
        let (code, exit, shown) = refusal(&plan, &flags(true, Some("reviewed drift")));
        assert_eq!((code.as_str(), exit), ("deploy_blocked", 6));
        assert!(shown.contains("ignition/a/x"), "{shown}");
        assert!(!shown.contains("ignition/a/y"), "{shown}");
    }

    #[test]
    fn order_shadows_removal_beats_the_destructive_gate() {
        let mut shadowed = destructive("ignition/a/x");
        shadowed.core = CoreRelation::ShadowsRemoval;
        let plan = plan_of(vec![shadowed]);
        assert_eq!(refusal(&plan, &flags(false, None)).0, "deploy_blocked");
    }

    #[test]
    fn order_destructive_without_the_flag_is_refused() {
        let plan = plan_of(vec![destructive("ignition/a/x")]);
        let (code, exit, shown) = refusal(&plan, &flags(false, None));
        assert_eq!((code.as_str(), exit), ("destructive_change_refused", 2));
        assert!(shown.contains("destructive"), "{shown}");
        assert!(shown.contains("ignition/a/x"), "{shown}");
    }

    #[test]
    fn order_drift_without_the_reason_is_refused() {
        let plan = plan_of(vec![drifted("ignition/a/x")]);
        let (code, exit, shown) = refusal(&plan, &flags(true, None));
        assert_eq!((code.as_str(), exit), ("destructive_change_refused", 2));
        assert!(shown.contains("drift"), "{shown}");
        assert!(shown.contains("ignition/a/x"), "{shown}");
    }

    #[test]
    fn order_destructive_is_judged_before_drift() {
        let plan = plan_of(vec![drifted("ignition/a/d"), destructive("ignition/a/x")]);
        let (_, _, shown) = refusal(&plan, &flags(false, None));
        assert!(shown.contains("ignition/a/x"), "{shown}");
        assert!(!shown.contains("ignition/a/d"), "{shown}");
        let (_, _, shown) = refusal(&plan, &flags(true, None));
        assert!(shown.contains("ignition/a/d"), "{shown}");
    }

    #[test]
    fn order_both_flags_make_it_ready() {
        let plan = plan_of(vec![destructive("ignition/a/x"), drifted("ignition/a/y")]);
        let outcome = evaluate_gates(&plan, &flags(true, Some("reviewed on the gateway")))
            .expect("both flags");
        assert!(matches!(outcome, GateOutcome::Ready { .. }), "{outcome:?}");
    }

    #[test]
    fn unmanaged_entries_never_gate() {
        let mut odd = entry("ignition/a/u", PlanStatus::Unmanaged);
        odd.destructive = true;
        odd.drift = Some(true);
        odd.core = CoreRelation::ShadowsRemoval;
        let plan = plan_of(vec![odd, entry("ignition/a/x", PlanStatus::Added)]);
        let outcome = evaluate_gates(&plan, &flags(false, None)).expect("ready");
        assert!(matches!(outcome, GateOutcome::Ready { .. }), "{outcome:?}");
    }

    #[test]
    fn drift_none_and_false_never_gate() {
        let mut no_drift = entry("ignition/a/x", PlanStatus::Changed);
        no_drift.drift = Some(false);
        let plan = plan_of(vec![no_drift, entry("ignition/a/y", PlanStatus::Added)]);
        let outcome = evaluate_gates(&plan, &flags(false, None)).expect("ready");
        assert!(matches!(outcome, GateOutcome::Ready { .. }), "{outcome:?}");
    }

    #[test]
    fn refusal_keys_are_byte_sorted_and_capped_at_ten() {
        let mut entries: Vec<PlanEntry> = (0..12)
            .rev()
            .map(|i| destructive(&format!("ignition/a/k{i:02}")))
            .collect();
        entries.push(entry("ignition/a/zz", PlanStatus::Added));
        let plan = plan_of(entries);
        let (_, _, shown) = refusal(&plan, &flags(false, None));
        assert!(shown.contains("12"), "{shown}");
        assert!(shown.contains("and 2 more"), "{shown}");
        let first = shown.find("ignition/a/k00").expect("k00 shown");
        let tenth = shown.find("ignition/a/k09").expect("k09 shown");
        assert!(first < tenth, "{shown}");
        assert!(!shown.contains("ignition/a/k10"), "{shown}");
        assert!(!shown.contains("ignition/a/zz"), "{shown}");
    }

    #[test]
    fn a_malformed_review_hash_is_invalid_input_not_stale() {
        let plan = plan_of(vec![entry("ignition/a/x", PlanStatus::Added)]);
        let mut f = flags(false, None);
        f.plan_sha256 = Some("NOT-A-HASH".to_string());
        let (code, exit, _) = refusal(&plan, &f);
        assert_eq!((code.as_str(), exit), ("invalid_input", 2));
    }

    #[test]
    fn drift_reason_is_validated_on_every_evaluation() {
        let plan = plan_of(vec![entry("ignition/a/x", PlanStatus::Added)]);
        for bad in ["", "two\nlines", "nul\0byte"] {
            let (code, exit, _) = refusal(&plan, &flags(false, Some(bad)));
            assert_eq!((code.as_str(), exit), ("invalid_input", 2), "{bad:?}");
        }
    }

    #[test]
    fn drift_reason_rules() {
        assert!(validate_drift_reason(&"r".repeat(200)).is_ok());
        assert!(validate_drift_reason(&"r".repeat(DRIFT_REASON_MAX_CHARS)).is_ok());
        for bad in [
            String::new(),
            "r".repeat(DRIFT_REASON_MAX_CHARS + 1),
            "a\nb".to_string(),
            "a\rb".to_string(),
            "a\0b".to_string(),
            "a\u{1b}[31mb".to_string(),
        ] {
            let err = validate_drift_reason(&bad).expect_err(&bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
            assert_eq!(err.exit_code(), 2);
        }
    }

    #[test]
    fn drift_reason_length_counts_characters_not_bytes() {
        assert!(validate_drift_reason(&"\u{e9}".repeat(DRIFT_REASON_MAX_CHARS)).is_ok());
    }

    #[test]
    fn prose_is_stable_and_carries_no_content() {
        let mut d = destructive("ignition/a/x");
        d.diff = Some("SECRET-DIFF-BYTES".to_string());
        let plan = plan_of(vec![
            entry("ignition/a/y", PlanStatus::Added),
            d,
            entry("ignition/a/s", PlanStatus::Same),
        ]);
        let one = confirmation_prose(&plan);
        assert_eq!(one, confirmation_prose(&plan));
        assert!(one.contains(HASH), "{one}");
        assert!(one.contains("prod") && one.contains("gw") && one.contains("production"));
        assert!(
            one.contains("ignition/a/y") && one.contains("ignition/a/x"),
            "{one}"
        );
        assert!(
            !one.contains("ignition/a/s"),
            "same entries are not listed: {one}"
        );
        assert!(!one.contains("SECRET-DIFF-BYTES"), "{one}");
        assert!(one.contains("--yes"), "{one}");
    }
}
