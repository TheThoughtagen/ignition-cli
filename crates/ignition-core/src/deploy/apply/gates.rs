//! Pure gate evaluation and refusal prose (DAPL-01, DAPL-02).
//!
//! [`evaluate_gates`] decides, from a plan and the caller's flags alone,
//! whether an apply may proceed. It reads no host, no gateway and no clock.

use crate::actions::deploy::DeployPlanResult;
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
pub fn evaluate_gates(
    _plan: &DeployPlanResult,
    _flags: &GateFlags,
) -> Result<GateOutcome, CoreError> {
    Ok(GateOutcome::Noop)
}

/// The plan as the refusal text.
pub fn confirmation_prose(_plan: &DeployPlanResult) -> String {
    String::new()
}

/// `--plan-sha256` must be 64 lowercase hex characters.
pub fn validate_plan_sha256(_value: &str) -> Result<(), CoreError> {
    Ok(())
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
