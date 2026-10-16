//! Pure gate evaluation and refusal prose (DAPL-01, DAPL-02).
//!
//! [`evaluate_gates`] decides, from a plan and the caller's flags alone,
//! whether an apply may proceed. It reads no host, no gateway and no clock.

use crate::actions::deploy::DeployPlanResult;
use crate::deploy::plan::{CoreRelation, PlanStatus};
use crate::error::CoreError;

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
/// A `--plan-sha256` that differs from the recomputed `plan_sha256` is
/// `plan_stale` before anything else, so a flag the reviewer gave for an old
/// plan can never satisfy a gate of a new one.
pub fn evaluate_gates(
    plan: &DeployPlanResult,
    flags: &GateFlags,
) -> Result<GateOutcome, CoreError> {
    if let Some(expected) = &flags.plan_sha256
        && *expected != plan.plan_sha256
    {
        return Err(CoreError::PlanStale {
            expected: expected.clone(),
            actual: plan.plan_sha256.clone(),
            detail: "review hash".to_string(),
        });
    }
    Ok(GateOutcome::Ready {
        prose: confirmation_prose(plan),
    })
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
    use super::validate_plan_sha256;

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
}
