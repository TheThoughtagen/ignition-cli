//! Excluded tag providers: gateway-owned providers no deploy verb touches.
//!
//! Some tag providers belong to the gateway, not to the declared tree: the
//! built-in `System` provider, and providers a module generates (Cirrus Link's
//! `MQTT Engine` builds its tags from Sparkplug births). An excluded
//! provider's four [`TAG_TYPES`] subtrees (`ignition/<tag type>/<P>/`) are
//! never listed, read, written or deleted by `deploy pull`, `plan`, `verify`
//! or `apply`.
//!
//! - **The effective set** is the union of [`DEFAULT_EXCLUDED_TAG_PROVIDERS`]
//!   (`System`), the target's `exclude_tag_providers` and the manifest's
//!   `exclude_tag_providers` ([`effective`]). The `System` default cannot be
//!   turned off in this release: no target or manifest field removes it.
//! - **Matching folds ASCII case** ([`is_excluded`]). Ignition provider names
//!   and NTFS paths are case-insensitive, so a declared `system` is the
//!   excluded `System`. This is an assumption about the gateway, stated here
//!   so a change to it is a visible decision.
//! - **Contradictions are refused.** A provider that is excluded and also
//!   declared (manifest `tag_providers`, or a `tags/<P>/` tree) is a
//!   `deploy_tree_invalid` finding ([`contradictions`], and render's policy
//!   stage).
//! - **Never deleted.** A ledger file under an excluded provider never enters
//!   plan's delete set, even from a ledger written before the exclusion
//!   ([`excluded_provider`]).
//! - **Not hashed.** The effective set is not part of `plan_sha256`; it is
//!   reported in the additive `excluded_tag_providers` data key of the four
//!   verbs and on one human-mode stderr line.

use std::collections::BTreeMap;

use super::component_problem;
use super::layout::{RelPath, TAG_TYPES};
use super::manifest::Manifest;
use super::target::Target;
use crate::error::DeployFinding;

/// The providers excluded without being listed. Always part of the effective
/// set; this release has no way to turn the default off.
pub const DEFAULT_EXCLUDED_TAG_PROVIDERS: [&str; 1] = ["System"];

/// Why `name` cannot be an `exclude_tag_providers` entry, or `None`.
///
/// [`component_problem`], plus a blank entry and an entry with leading or
/// trailing whitespace: a padded name would silently match no provider.
pub fn entry_problem(name: &str) -> Option<&'static str> {
    if !name.is_empty() && name.trim().is_empty() {
        Some("must not be blank")
    } else if name.trim() != name {
        Some("must not have leading or trailing whitespace")
    } else {
        component_problem(name)
    }
}

/// The findings of an `exclude_tag_providers` list: each bad entry, and each
/// entry equal to an earlier one under ASCII case folding (duplicates are
/// refused, not normalized). Messages are `exclude_tag_providers[i] "X": why`.
pub(crate) fn list_problems(list: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    for (i, name) in list.iter().enumerate() {
        if let Some(why) = entry_problem(name) {
            problems.push(format!("exclude_tag_providers[{i}] {name:?}: {why}"));
        }
        if list[..i]
            .iter()
            .any(|earlier| earlier.eq_ignore_ascii_case(name))
        {
            problems.push(format!(
                "exclude_tag_providers[{i}] {name:?}: duplicate tag provider"
            ));
        }
    }
    problems
}

/// The effective exclusion set: the built-in default, then the target's
/// list, then the manifest's (absent before a pull has a manifest).
/// Deduplicated by ASCII case fold, keeping the byte-smallest spelling;
/// byte-sorted. Always holds `System`.
pub fn effective(target: &Target, manifest: Option<&Manifest>) -> Vec<String> {
    let mut by_fold: BTreeMap<String, String> = BTreeMap::new();
    let listed = DEFAULT_EXCLUDED_TAG_PROVIDERS
        .iter()
        .map(|p| (*p).to_string())
        .chain(target.exclude_tag_providers.iter().cloned())
        .chain(
            manifest
                .into_iter()
                .flat_map(|m| m.exclude_tag_providers.iter().cloned()),
        );
    for name in listed {
        let fold = name.to_ascii_lowercase();
        match by_fold.get_mut(&fold) {
            Some(kept) if name.as_bytes() < kept.as_bytes() => *kept = name,
            Some(_) => {}
            None => {
                by_fold.insert(fold, name);
            }
        }
    }
    let mut names: Vec<String> = by_fold.into_values().collect();
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    names
}

/// Whether `provider` is in `excluded`, under ASCII case folding.
pub fn is_excluded(provider: &str, excluded: &[String]) -> bool {
    excluded.iter().any(|e| e.eq_ignore_ascii_case(provider))
}

/// The provider component of a collection-relative `path` under one of the
/// four tag type directories of an excluded provider
/// (`ignition/<tag type>/<P>/...` with `P` excluded), spelled as in `path`;
/// `None` for any other path.
pub fn excluded_provider<'a>(path: &'a RelPath, excluded: &[String]) -> Option<&'a str> {
    let mut parts = path.components();
    let (module, ty, provider) = (parts.next()?, parts.next()?, parts.next()?);
    (module == "ignition" && TAG_TYPES.contains(&ty) && is_excluded(provider, excluded))
        .then_some(provider)
}

/// One finding per `manifest.tag_providers[i]` that is excluded, labelled
/// `manifest_file`, with no line or column.
pub fn contradictions(
    excluded: &[String],
    manifest: &Manifest,
    manifest_file: &str,
) -> Vec<DeployFinding> {
    manifest
        .tag_providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| is_excluded(provider, excluded))
        .map(|(i, provider)| DeployFinding {
            file: manifest_file.to_string(),
            line: None,
            column: None,
            message: format!(
                "tag_providers[{i}] {provider:?}: the tag provider is excluded (by exclude_tag_providers or the built-in System default); an excluded provider cannot be declared"
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(extra: &str) -> Target {
        let yaml = format!(
            "schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n{extra}"
        );
        Target::parse("targets/dev.yaml", yaml.as_bytes()).expect("target")
    }

    fn manifest(extra: &str) -> Manifest {
        let yaml = format!("schema_version: 1\ntypes: []\n{extra}");
        Manifest::parse("gateways/g/manifest.yaml", yaml.as_bytes()).expect("manifest")
    }

    #[test]
    fn exclude_default_is_system_alone() {
        assert_eq!(DEFAULT_EXCLUDED_TAG_PROVIDERS, ["System"]);
        assert_eq!(effective(&target(""), None), ["System"]);
        assert_eq!(effective(&target(""), Some(&manifest(""))), ["System"]);
    }

    #[test]
    fn exclude_effective_unions_default_target_and_manifest() {
        let t = target("exclude_tag_providers: [\"MQTT Engine\"]\n");
        let m = manifest("exclude_tag_providers: [\"Sparkplug B\", system]\n");
        // fold dedupe keeps the byte-smallest spelling: `System` < `system`
        assert_eq!(
            effective(&t, Some(&m)),
            ["MQTT Engine", "Sparkplug B", "System"]
        );
        // overlap across sources is unioned, not refused
        let both = manifest("exclude_tag_providers: [\"MQTT Engine\"]\n");
        assert_eq!(effective(&t, Some(&both)), ["MQTT Engine", "System"]);
    }

    #[test]
    fn exclude_is_excluded_folds_ascii_case() {
        let excluded = vec!["MQTT Engine".to_string(), "System".to_string()];
        assert!(is_excluded("system", &excluded));
        assert!(is_excluded("mqtt ENGINE", &excluded));
        assert!(!is_excluded("default", &excluded));
        assert!(!is_excluded("MQTT", &excluded));
    }

    #[test]
    fn exclude_contradictions_flag_declared_excluded_providers() {
        let m = manifest("tag_providers: [default, system]\n");
        let found = contradictions(
            &effective(&target(""), Some(&m)),
            &m,
            "gateways/g/manifest.yaml",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].file, "gateways/g/manifest.yaml");
        assert_eq!((found[0].line, found[0].column), (None, None));
        assert!(
            found[0].message.starts_with("tag_providers[1] \"system\"")
                && found[0].message.contains("excluded"),
            "{}",
            found[0].message
        );

        let t = target("exclude_tag_providers: [\"MQTT Engine\"]\n");
        let m = manifest("tag_providers: [\"MQTT Engine\", default]\n");
        let found = contradictions(&effective(&t, Some(&m)), &m, "gateways/g/manifest.yaml");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0]
                .message
                .starts_with("tag_providers[0] \"MQTT Engine\""),
            "{}",
            found[0].message
        );

        let m = manifest("tag_providers: [default]\n");
        assert!(contradictions(&effective(&t, Some(&m)), &m, "x").is_empty());
    }

    #[test]
    fn exclude_excluded_provider_matches_the_four_tag_types_only() {
        let excluded = vec!["MQTT Engine".to_string(), "System".to_string()];
        let of = |p: &str| {
            let path = RelPath::new(p).expect("path");
            excluded_provider(&path, &excluded).map(str::to_string)
        };
        for (path, want) in [
            (
                "ignition/tag-provider/MQTT Engine/config.json",
                Some("MQTT Engine"),
            ),
            (
                "ignition/tag-group/System/Default/config.json",
                Some("System"),
            ),
            (
                "ignition/tag-type-definition/system/T/tags.json",
                Some("system"),
            ),
            (
                "ignition/tag-definition/mqtt engine/Edge Nodes/tags.json",
                Some("mqtt engine"),
            ),
            ("ignition/tag-provider/default/config.json", None),
            ("ignition/database-translator/MQTT Engine/config.json", None),
            ("com.example/tag-provider/MQTT Engine/config.json", None),
        ] {
            assert_eq!(of(path).as_deref(), want, "{path}");
        }
    }

    #[test]
    fn exclude_entry_problem_refuses_blank_padded_and_unsafe_names() {
        assert_eq!(entry_problem("MQTT Engine"), None);
        assert_eq!(entry_problem("System"), None);
        for bad in [
            "",
            "   ",
            " MQTT Engine",
            "MQTT Engine\t",
            "a/b",
            ".x",
            "..",
        ] {
            assert!(entry_problem(bad).is_some(), "{bad:?}");
        }
    }
}
