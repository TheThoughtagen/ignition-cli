//! Pure gate evaluation and refusal prose (DAPL-01, DAPL-02).
//!
//! [`evaluate_gates`] decides, from a plan, the caller's flags and whether
//! the target is a redundant pair ([`Redundancy`]) alone, whether an apply may
//! proceed. It reads no host, no gateway and no clock.

use crate::actions::deploy::DeployPlanResult;
use crate::deploy::plan::{CoreRelation, PlanEntry, PlanStatus, pair_tag_keys};
pub use crate::deploy::target::Redundancy;
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

/// The `deploy_pair_tags_refused` refusal for `plan` on a target of
/// `redundancy` (debug deploy-tag-redundancy-sync), or `None`: on a pair,
/// every [`pair_tag_keys`] entry, the residue a routed apply cannot delete
/// from the mode collection (quick 261102-cef, D3). The one definition both
/// [`evaluate_gates`] and the engine's own entry check
/// ([`run_apply`](super::run_apply), the WR-04 pattern) use, so the two
/// cannot drift. A standalone gateway loads tag units from its mode
/// collection normally, so it is never refused.
pub fn pair_tags_refusal(plan: &DeployPlanResult, redundancy: Redundancy) -> Option<CoreError> {
    if redundancy == Redundancy::Standalone {
        return None;
    }
    let keys = pair_tag_keys(&plan.entries, &plan.delete_set);
    if keys.is_empty() {
        return None;
    }
    Some(CoreError::DeployPairTagsRefused {
        mode: plan.mode.clone(),
        keys,
    })
}

/// The `deploy_unreadable` refusal for `plan` (quick-261029-ms3), or `None`:
/// every unreadable unit the plan's master read found that is declared or in
/// the declared-names ledger, byte-sorted and unique. The one definition both
/// [`evaluate_gates`] and the engine's own entry check
/// ([`run_apply`](super::run_apply), WR-04) use, so the two cannot drift.
pub fn unreadable_refusal(plan: &DeployPlanResult) -> Option<CoreError> {
    let mut keys: Vec<String> = plan
        .unreadable
        .iter()
        .filter(|u| u.declared || u.in_ledger)
        .map(|u| u.key.clone())
        .collect();
    if keys.is_empty() {
        return None;
    }
    keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    keys.dedup();
    Some(CoreError::DeployUnreadable {
        node: "master".to_string(),
        keys,
    })
}

/// Evaluate the gates in the confirmed order.
///
/// 1. A malformed flag is `invalid_input` (exit 2), whatever the plan says.
/// 2. A `--plan-sha256` that differs from the recomputed `plan_sha256` is
///    `plan_stale` (exit 6), so a flag the reviewer gave for an old plan can
///    never satisfy a gate of a new one.
/// 3. An unreadable unit (quick-261029-ms3) that is declared or in the
///    declared-names ledger is `deploy_unreadable` (exit 6), whatever the
///    flags. This runs before the no-op return, so a plan whose `has_changes`
///    is false can never carry such a unit through to an apply.
/// 4. On a redundant pair, a `tag-definition` or `tag-type-definition` unit
///    whose mode-collection files the apply would leave behind
///    ([`pair_tags_refusal`]: files the delete set does not clear) is
///    `deploy_pair_tags_refused` (exit 6), whatever the flags, also before the
///    no-op return: such a file already stops the backup from syncing. A unit
///    the plan routes to `core` (quick 261102-cef, A2) passes, `added`,
///    `changed` and `same` alike, and so does one whose mode copy the delete
///    set clears (the migration, or the removal).
/// 5. A plan with nothing to add, change or remove is [`GateOutcome::Noop`].
/// 6. Any entry whose `core` copy shadows its removal is `deploy_blocked`
///    (exit 6), whatever the flags.
/// 7. Destructive entries without `--allow-destructive`, then drifted
///    entries without `--allow-drift`, are `destructive_change_refused`
///    (exit 2).
/// 8. Otherwise [`GateOutcome::Ready`].
///
/// `unmanaged` entries never gate, and drift is judged only where
/// `drift == Some(true)`, which exists only with `--base`. An unreadable
/// unit that is neither declared nor in the ledger never gates either; its
/// plan warning reaches the apply result.
pub fn evaluate_gates(
    plan: &DeployPlanResult,
    flags: &GateFlags,
    redundancy: Redundancy,
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
    if let Some(refusal) = unreadable_refusal(plan) {
        return Err(refusal);
    }
    if let Some(refusal) = pair_tags_refusal(plan, redundancy) {
        return Err(refusal);
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
        DRIFT_REASON_MAX_CHARS, GateFlags, GateOutcome, Redundancy, confirmation_prose,
        evaluate_gates, pair_tags_refusal, validate_drift_reason, validate_plan_sha256,
    };
    use crate::actions::deploy::DeployPlanResult;
    use crate::deploy::layout::RelPath;
    use crate::deploy::plan::{
        CoreRelation, PlanEntry, PlanFile, PlanStatus, PlanSummary, UnreadableUnit,
    };
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
            excluded_tag_providers: vec!["System".to_string()],
            unreadable: Vec::new(),
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
        let err =
            evaluate_gates(plan, flags, Redundancy::Standalone).expect_err("the gates refuse");
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
            evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone).expect("noop"),
            GateOutcome::Noop
        );
        let mut matching = flags(false, None);
        matching.plan_sha256 = Some(HASH.to_string());
        assert_eq!(
            evaluate_gates(&plan, &matching, Redundancy::Standalone)
                .expect("noop with its own hash"),
            GateOutcome::Noop
        );
    }

    fn unreadable(key: &str, declared: bool, in_ledger: bool) -> UnreadableUnit {
        UnreadableUnit {
            key: key.to_string(),
            live_in: vec!["mode".to_string()],
            declared,
            in_ledger,
            paths: vec![format!("{key}/config.json")],
            longest: 262,
        }
    }

    /// A plan carrying `units`, with `has_changes` and `summary.unreadable`
    /// set the way the plan engine sets them (P6).
    fn with_unreadable(mut plan: DeployPlanResult, units: Vec<UnreadableUnit>) -> DeployPlanResult {
        plan.summary.unreadable = units.len();
        plan.summary.has_changes |= units.iter().any(|u| u.declared || u.in_ledger);
        plan.unreadable = units;
        plan
    }

    #[test]
    fn unreadable_declared_unit_is_refused_deploy_unreadable() {
        let plan = with_unreadable(
            plan_of(vec![entry("ignition/a/x", PlanStatus::Same)]),
            vec![unreadable(
                "ignition/tag-definition/default/Deep",
                true,
                false,
            )],
        );
        assert!(plan.summary.has_changes);
        let (code, exit, text) = refusal(&plan, &flags(true, Some("ok")));
        assert_eq!((code.as_str(), exit), ("deploy_unreadable", 6));
        assert!(
            text.contains("ignition/tag-definition/default/Deep"),
            "{text}"
        );
    }

    #[test]
    fn unreadable_in_ledger_unit_is_refused_before_the_noop_return() {
        let mut plan = with_unreadable(
            plan_of(vec![entry("ignition/a/x", PlanStatus::Same)]),
            vec![unreadable("ignition/database-translator/OLD", false, true)],
        );
        plan.summary.has_changes = false;
        let (code, exit, _) = refusal(&plan, &flags(false, None));
        assert_eq!((code.as_str(), exit), ("deploy_unreadable", 6));
    }

    #[test]
    fn unreadable_keys_are_byte_sorted_and_only_the_ones_that_matter() {
        let plan = with_unreadable(
            plan_of(Vec::new()),
            vec![
                unreadable("ignition/b/z", true, false),
                unreadable("ignition/a/undeclared", false, false),
                unreadable("ignition/B/y", false, true),
            ],
        );
        let err = evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone)
            .expect_err("refused");
        match err {
            crate::error::CoreError::DeployUnreadable { node, keys } => {
                assert_eq!(node, "master");
                assert_eq!(keys, vec!["ignition/B/y", "ignition/b/z"]);
            }
            other => panic!("expected DeployUnreadable, got {other:?}"),
        }
    }

    #[test]
    fn unreadable_stale_review_hash_is_still_plan_stale_first() {
        let plan = with_unreadable(
            plan_of(Vec::new()),
            vec![unreadable("ignition/a/x", true, true)],
        );
        let mut f = flags(false, None);
        f.plan_sha256 = Some("1".repeat(64));
        assert_eq!(refusal(&plan, &f).0, "plan_stale");
    }

    #[test]
    fn unreadable_undeclared_unledgered_unit_is_noop_when_nothing_else_changes() {
        let plan = with_unreadable(
            plan_of(vec![entry("ignition/a/x", PlanStatus::Same)]),
            vec![unreadable(
                "ignition/tag-definition/MQTT/Edge",
                false,
                false,
            )],
        );
        assert_eq!(
            evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone).expect("noop"),
            GateOutcome::Noop
        );
    }

    #[test]
    fn unreadable_undeclared_unledgered_unit_is_ready_when_another_entry_changes() {
        let plan = with_unreadable(
            plan_of(vec![entry("ignition/a/x", PlanStatus::Changed)]),
            vec![unreadable(
                "ignition/tag-definition/MQTT/Edge",
                false,
                false,
            )],
        );
        assert!(matches!(
            evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone).expect("ready"),
            GateOutcome::Ready { .. }
        ));
    }

    fn tag_entry(key: &str, ty: &str, status: PlanStatus, live_in: &[&str]) -> PlanEntry {
        let mut e = entry(key, status);
        e.ty = ty.to_string();
        e.live_in = live_in.iter().map(|c| (*c).to_string()).collect();
        e
    }

    /// `e` with `names` as its live files under its key (a mode copy).
    fn with_live_files(mut e: PlanEntry, names: &[&str]) -> PlanEntry {
        e.files = names
            .iter()
            .map(|name| PlanFile {
                path: format!("{}/{name}", e.key),
                live_sha256: Some("cd".repeat(32)),
                desired_sha256: None,
            })
            .collect();
        e
    }

    /// `plan` with `paths` as its delete set.
    fn deleting(mut plan: DeployPlanResult, paths: &[String]) -> DeployPlanResult {
        plan.delete_set = DeleteSet {
            files: paths
                .iter()
                .map(|p| RelPath::new(p).expect("a valid path"))
                .collect(),
        };
        plan
    }

    /// `e` with `.core/` rows for `names` under its key (a routed unit's core
    /// copy and render).
    fn with_core_rows(mut e: PlanEntry, names: &[&str]) -> PlanEntry {
        e.files.extend(names.iter().map(|name| PlanFile {
            path: format!(".core/{}/{name}", e.key),
            live_sha256: Some("ab".repeat(32)),
            desired_sha256: Some("ab".repeat(32)),
        }));
        e.files
            .sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        e
    }

    /// quick 261102-cef (A2, D3): on a pair a routed tag unit is written into
    /// `core`, so `added`, `changed` and `same` units pass, `.core/` rows
    /// never count, and only the residue is refused: a mode-collection file
    /// (a bare row with a live sha) the delete set does not clear, of a unit
    /// that is not `unmanaged`. A standalone target is never refused.
    #[test]
    fn pair_tags_written_or_kept_in_the_mode_collection_are_refused_on_a_pair_only() {
        let plan = plan_of(vec![
            with_core_rows(
                tag_entry(
                    "ignition/tag-definition/default/Added",
                    "tag-definition",
                    PlanStatus::Added,
                    &[],
                ),
                &["tags.json", "unary-resource.json"],
            ),
            with_core_rows(
                tag_entry(
                    "ignition/tag-type-definition/default/Changed",
                    "tag-type-definition",
                    PlanStatus::Changed,
                    &["core"],
                ),
                &["udts.json"],
            ),
            with_core_rows(
                tag_entry(
                    "ignition/tag-definition/default/CoreOnly",
                    "tag-definition",
                    PlanStatus::Same,
                    &["core"],
                ),
                &["tags.json"],
            ),
            // residue: a mode copy nobody deletes, beside a core copy
            with_live_files(
                tag_entry(
                    "ignition/tag-definition/default/KeptInMode",
                    "tag-definition",
                    PlanStatus::Same,
                    &["core", "mode"],
                ),
                &["tags.json"],
            ),
            with_live_files(
                tag_entry(
                    "ignition/tag-definition/default/Unmanaged",
                    "tag-definition",
                    PlanStatus::Unmanaged,
                    &["mode"],
                ),
                &["tags.json"],
            ),
            with_live_files(
                tag_entry(
                    "ignition/tag-group/default/Fast",
                    "tag-group",
                    PlanStatus::Same,
                    &["mode"],
                ),
                &["config.json"],
            ),
            entry("ignition/a/x", PlanStatus::Added),
        ]);
        let both = flags(true, Some("reviewed"));
        let err = evaluate_gates(&plan, &both, Redundancy::Pair).expect_err("refused");
        assert_eq!(
            (err.code(), err.exit_code()),
            ("deploy_pair_tags_refused", 6)
        );
        match &err {
            crate::error::CoreError::DeployPairTagsRefused { mode, keys } => {
                assert_eq!(mode, "production");
                assert_eq!(
                    keys,
                    &vec!["ignition/tag-definition/default/KeptInMode".to_string()]
                );
            }
            other => panic!("expected DeployPairTagsRefused, got {other:?}"),
        }
        assert!(matches!(
            evaluate_gates(&plan, &both, Redundancy::Standalone).expect("standalone"),
            GateOutcome::Ready { .. }
        ));
        assert!(pair_tags_refusal(&plan, Redundancy::Standalone).is_none());

        // without the residue, every routed unit passes on the pair
        let mut clean = plan.clone();
        clean
            .entries
            .retain(|e| e.key != "ignition/tag-definition/default/KeptInMode");
        assert!(pair_tags_refusal(&clean, Redundancy::Pair).is_none());
        assert!(matches!(
            evaluate_gates(&clean, &both, Redundancy::Pair).expect("ready"),
            GateOutcome::Ready { .. }
        ));
    }

    /// A plan made unrouted (for a standalone target) would write its tag
    /// units into the mode collection: on a pair it is refused, whatever its
    /// status, so such a plan never reaches a pair.
    #[test]
    fn pair_tags_unrouted_plan_is_refused_on_a_pair() {
        let mut added = tag_entry(
            "ignition/tag-definition/default/Deploy",
            "tag-definition",
            PlanStatus::Added,
            &[],
        );
        added.files = vec![PlanFile {
            path: "ignition/tag-definition/default/Deploy/tags.json".to_string(),
            live_sha256: None,
            desired_sha256: Some("ab".repeat(32)),
        }];
        let plan = plan_of(vec![added]);
        let err = pair_tags_refusal(&plan, Redundancy::Pair).expect("refused");
        assert_eq!(err.code(), "deploy_pair_tags_refused");
        assert!(pair_tags_refusal(&plan, Redundancy::Standalone).is_none());
    }

    /// A mode-collection file a pair's plan keeps already stops the backup
    /// from syncing (T6), so the residue is refused before the no-op return,
    /// like an unreadable unit; the same unit in `core` alone is a no-op.
    #[test]
    fn pair_tags_kept_unit_is_refused_before_the_noop_return() {
        let kept = with_live_files(
            with_core_rows(
                tag_entry(
                    "ignition/tag-definition/default/Deploy",
                    "tag-definition",
                    PlanStatus::Same,
                    &["core", "mode"],
                ),
                &["tags.json"],
            ),
            &["tags.json"],
        );
        let plan = plan_of(vec![kept]);
        assert!(!plan.summary.has_changes);
        let (code, exit, text) = {
            let err =
                evaluate_gates(&plan, &flags(false, None), Redundancy::Pair).expect_err("refused");
            (err.code().to_string(), err.exit_code(), err.to_string())
        };
        assert_eq!((code.as_str(), exit), ("deploy_pair_tags_refused", 6));
        assert!(
            text.contains("ignition/tag-definition/default/Deploy"),
            "{text}"
        );
        assert_eq!(
            evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone).expect("noop"),
            GateOutcome::Noop
        );
        let core_only = plan_of(vec![with_core_rows(
            tag_entry(
                "ignition/tag-definition/default/Deploy",
                "tag-definition",
                PlanStatus::Same,
                &["core"],
            ),
            &["tags.json"],
        )]);
        assert_eq!(
            evaluate_gates(&core_only, &flags(false, None), Redundancy::Pair).expect("noop"),
            GateOutcome::Noop
        );
    }

    /// quick 261102-cef (D3): the migration, a routed unit whose mode copy
    /// the delete set clears, passes on a pair with its flag.
    #[test]
    fn pair_tags_migration_clearing_the_mode_copy_passes() {
        let mut moved = with_core_rows(
            with_live_files(
                tag_entry(
                    "ignition/tag-definition/default/Deploy",
                    "tag-definition",
                    PlanStatus::Added,
                    &["mode"],
                ),
                &["tags.json", "unary-resource.json"],
            ),
            &["tags.json", "unary-resource.json"],
        );
        moved.destructive = true;
        let paths: Vec<String> = moved
            .files
            .iter()
            .filter(|f| !f.path.starts_with(".core/"))
            .map(|f| f.path.clone())
            .collect();
        let plan = deleting(plan_of(vec![moved]), &paths);
        assert!(pair_tags_refusal(&plan, Redundancy::Pair).is_none());
        assert!(matches!(
            evaluate_gates(&plan, &flags(true, None), Redundancy::Pair).expect("ready"),
            GateOutcome::Ready { .. }
        ));
    }

    /// Deleting a tag unit from the mode collection is how a pair recovers
    /// (T1d, T6e): a `removed` tag entry whose mode files are all in the
    /// delete set passes on a pair with its flag.
    #[test]
    fn pair_tags_removed_unit_is_the_recovery_and_passes() {
        let mut removed = with_live_files(
            tag_entry(
                "ignition/tag-definition/default/Deploy",
                "tag-definition",
                PlanStatus::Removed,
                &["mode"],
            ),
            &["tags.json", "unary-resource.json"],
        );
        removed.destructive = true;
        let paths: Vec<String> = removed.files.iter().map(|f| f.path.clone()).collect();
        let plan = deleting(plan_of(vec![removed]), &paths);
        assert!(matches!(
            evaluate_gates(&plan, &flags(true, None), Redundancy::Pair).expect("ready"),
            GateOutcome::Ready { .. }
        ));
        assert!(pair_tags_refusal(&plan, Redundancy::Pair).is_none());
    }

    /// A `removed` tag unit whose mode copy keeps a file the delete set
    /// skips (here one the ledger does not list) keeps its folder in the mode
    /// collection, so the backup stays wedged (debug
    /// deploy-tag-redundancy-sync): refused on a pair, whatever the flags.
    #[test]
    fn pair_tags_removed_unit_leaving_a_mode_file_is_refused() {
        let key = "ignition/tag-definition/default/Deploy";
        let mut removed = with_live_files(
            tag_entry(key, "tag-definition", PlanStatus::Removed, &["mode"]),
            &["tags.json", "unary-resource.json", "udts.json"],
        );
        removed.destructive = true;
        let plan = deleting(
            plan_of(vec![removed]),
            &[
                format!("{key}/tags.json"),
                format!("{key}/unary-resource.json"),
            ],
        );
        let err = evaluate_gates(&plan, &flags(true, None), Redundancy::Pair).expect_err("refused");
        assert_eq!(
            (err.code(), err.exit_code()),
            ("deploy_pair_tags_refused", 6)
        );
        match &err {
            crate::error::CoreError::DeployPairTagsRefused { keys, .. } => {
                assert_eq!(keys, &vec![key.to_string()]);
            }
            other => panic!("expected DeployPairTagsRefused, got {other:?}"),
        }
        assert!(matches!(
            evaluate_gates(&plan, &flags(true, None), Redundancy::Standalone).expect("standalone"),
            GateOutcome::Ready { .. }
        ));
        assert!(pair_tags_refusal(&plan, Redundancy::Standalone).is_none());
    }

    /// The order: a stale review hash and an unreadable unit are named first.
    #[test]
    fn pair_tags_come_after_plan_stale_and_unreadable() {
        let tag = tag_entry(
            "ignition/tag-definition/default/Deploy",
            "tag-definition",
            PlanStatus::Added,
            &[],
        );
        let plan = plan_of(vec![tag.clone()]);
        let mut stale = flags(false, None);
        stale.plan_sha256 = Some("1".repeat(64));
        assert_eq!(
            evaluate_gates(&plan, &stale, Redundancy::Pair)
                .expect_err("stale")
                .code(),
            "plan_stale"
        );
        let plan = with_unreadable(
            plan_of(vec![tag]),
            vec![unreadable("ignition/database-translator/OLD", true, false)],
        );
        assert_eq!(
            evaluate_gates(&plan, &flags(true, Some("ok")), Redundancy::Pair)
                .expect_err("unreadable")
                .code(),
            "deploy_unreadable"
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
        let outcome = evaluate_gates(
            &plan,
            &flags(true, Some("reviewed on the gateway")),
            Redundancy::Standalone,
        )
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
        let outcome =
            evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone).expect("ready");
        assert!(matches!(outcome, GateOutcome::Ready { .. }), "{outcome:?}");
    }

    #[test]
    fn drift_none_and_false_never_gate() {
        let mut no_drift = entry("ignition/a/x", PlanStatus::Changed);
        no_drift.drift = Some(false);
        let plan = plan_of(vec![no_drift, entry("ignition/a/y", PlanStatus::Added)]);
        let outcome =
            evaluate_gates(&plan, &flags(false, None), Redundancy::Standalone).expect("ready");
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
