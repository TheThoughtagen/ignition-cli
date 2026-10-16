//! The guarded write half of `deploy` (Phase 23): gates, journal, snapshots,
//! the apply engine and rollback.
//!
//! This module and `deploy/transport/` are the only places that name the
//! write half. Every decision made here is pure or sits behind a seam, so a
//! test can drive it without a gateway. The vocabulary (error slugs and
//! exits, the gate order, the on-box journal, record and staging layouts, the
//! run-directory rule, the outcome, cause and preflight check ids) is the one
//! the user confirmed at the 23-02 checkpoint; see `23-02-SUMMARY.md`
//! `## Checkpoint outcome`.

pub mod gates;
pub mod run_store;

/// The slug of the rolled-back apply verdict.
///
/// Decision A: `deploy_rolled_back` rides the verdict payload (`data.slug`,
/// exit 6) and is never a [`crate::error::CoreError`] variant, never in
/// [`crate::error::CoreError::DEPLOY_SLUGS`] and never a row of the README
/// exit table. It follows the `deploy_verify_failed` precedent
/// ([`crate::deploy::verify::VERIFY_FAILED_SLUG`]).
pub const ROLLED_BACK_SLUG: &str = "deploy_rolled_back";
