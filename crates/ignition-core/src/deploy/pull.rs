//! The pull engine (Phase 22, DPLN-06): a live tree to a declared tree that
//! renders back to the live effective view.
//!
//! Pure: it takes an [`ObservedFiles`] the transport already read and returns
//! a [`SourceTree`]; it touches no filesystem, network or clock. The action
//! (`actions::deploy::deploy_pull`) reads the host through a
//! [`TreeReader`](super::transport::TreeReader) and writes the result.
//!
//! ## Layering (D-12)
//!
//! Files of the `core` collection go to the base tree (`resources/`,
//! `tags/`); files of the target's mode collection go to the
//! `modes/<env>/` overlay, so [`render`]'s overlay reproduces the gateway's
//! own layering. A unit in both collections lands in both. Bytes are kept
//! exactly as read, a bootstrapped resource keeps the gateway's uuid (D-12).
//!
//! ## The render fixpoint (D-09)
//!
//! A pulled tree is only worth writing if it is one the declared pipeline
//! accepts and reproduces. [`pull`] therefore renders its own tree and
//! repeats, dropping units, until a round drops nothing (at most
//! [`PULL_MAX_ROUNDS`] rounds):
//!
//! - a unit holding a secret location (Embedded or plaintext) is dropped with
//!   reason [`REASON_EMBEDDED_SECRET`];
//! - a unit any other render finding names is dropped with
//!   `render_refused: <finding message>`; the messages name paths and key
//!   names, never values;
//! - once render succeeds, a unit whose rendered files are not exactly its
//!   effective live files (mode copy when present, else `core`; same file
//!   set, same bytes) is dropped with [`REASON_NOT_ROUND_TRIPPABLE`]: a
//!   minted uuid, a dropped `.gitkeep`, a `core` file the mode copy lacks.
//!
//! A render error whose findings map to no unit (for example one naming
//! `manifest.yaml`) is returned as is: the manifest is the operator's to fix.
//! Because every kept unit renders byte-for-byte to its live copy, `pull`
//! then `render` then `plan` is empty, and every dropped unit is `unmanaged`
//! in the plan (the live copy exists, nothing renders it).
//!
//! ## Skip reasons (22-03 checkpoint, option-a)
//!
//! `embedded_secret`, `not_round_trippable`, `render_refused: <finding>`,
//! `type_without_rule`, `denied_type`. No reason ever carries a value.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::component_problem;
use super::layout::{RelPath, TAG_TYPES, repo_path};
use super::manifest::Manifest;
use super::plan::unit_key;
use super::render::{RenderInput, RenderOutcome, render};
use super::rules::{is_denied, type_rule};
use super::target::Target;
use super::transport::ObservedFiles;
use super::tree::SourceTree;
use crate::error::{CoreError, DeployFinding};

/// The most render rounds [`pull`] runs before it gives up.
pub const PULL_MAX_ROUNDS: usize = 8;

/// Skip reason: the unit holds an Embedded or plaintext secret location.
pub const REASON_EMBEDDED_SECRET: &str = "embedded_secret";
/// Skip reason: render does not reproduce the unit's live files.
pub const REASON_NOT_ROUND_TRIPPABLE: &str = "not_round_trippable";
/// Skip reason prefix: render refused the unit; the finding follows.
pub const REASON_RENDER_REFUSED: &str = "render_refused";
/// Skip reason: a live type with no render rule.
pub const REASON_TYPE_WITHOUT_RULE: &str = "type_without_rule";
/// Skip reason: a live type on the deny list.
pub const REASON_DENIED_TYPE: &str = "denied_type";

/// One live unit (or type) pull did not write, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    /// The unit key (`<module>/<type>/<name>`, tag folders
    /// `ignition/<tag type>/<provider>/<folder>`), or `<module>/<type>` for a
    /// type left out of a generated manifest. The same key `plan` reports.
    pub path: String,
    /// One of the reasons in the module doc. Never a value.
    pub reason: String,
}

/// What [`pull`] needs.
#[derive(Debug, Clone, Copy)]
pub struct PullInput<'a> {
    /// Gateway directory name under `gateways/`.
    pub gateway: &'a str,
    /// The environment (`--env`): names the `modes/<env>/` overlay.
    pub env: &'a str,
    /// The parsed target (render lints consult it).
    pub target: &'a Target,
    /// The manifest the pulled tree is rendered under.
    pub manifest: &'a Manifest,
    /// The live files: `core` and the target's mode collection.
    pub observed: &'a ObservedFiles,
}

/// What [`pull`] produced.
#[derive(Debug, Clone)]
pub struct PullOutcome {
    /// The tree to write under `gateways/<gateway>/`, gateway-relative.
    pub source: SourceTree,
    /// The render of `source`: what `deploy render` will produce from it.
    pub rendered: RenderOutcome,
    /// Every live unit not written, byte-sorted by path.
    pub skipped: Vec<Skipped>,
}

fn refused(message: &str) -> String {
    format!("{REASON_RENDER_REFUSED}: {message}")
}

/// Quote a scalar for the generated manifest only when a plain scalar could
/// be read as another YAML type or break the line.
fn yaml_scalar(value: &str) -> String {
    let plain = value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n"
        );
    if plain {
        value.to_string()
    } else {
        // a JSON string is a valid YAML double-quoted scalar
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    }
}

/// The `manifest.yaml` text for a gateway that has none (D-10), and the live
/// types it leaves out.
///
/// `live_types` are the `(module, type)` directories found in `core` and the
/// mode collection; `tag_providers` the provider names under
/// `ignition/tag-provider/`. The manifest lists, byte-sorted, every live type
/// that has a render rule and is not on the deny list, and none of the four
/// tag types (they are carried by `tag_providers`). Left out, with a reason:
/// denied types, types without a rule, and names that are not safe path
/// components. The text parses with `Manifest::parse`.
pub fn generate_manifest(
    live_types: &[(String, String)],
    tag_providers: &[String],
) -> (String, Vec<Skipped>) {
    let mut types: BTreeSet<String> = BTreeSet::new();
    let mut skipped: BTreeMap<String, String> = BTreeMap::new();
    for (module, ty) in live_types {
        if module == "ignition" && TAG_TYPES.contains(&ty.as_str()) {
            continue;
        }
        let id = format!("{module}/{ty}");
        if let Some(why) = component_problem(module).or_else(|| component_problem(ty)) {
            skipped.insert(id, refused(&format!("the type name {why}")));
        } else if is_denied(module, ty) {
            skipped.insert(id, REASON_DENIED_TYPE.to_string());
        } else if type_rule(module, ty).is_none() {
            skipped.insert(id, REASON_TYPE_WITHOUT_RULE.to_string());
        } else {
            types.insert(id);
        }
    }
    let mut providers: BTreeSet<String> = BTreeSet::new();
    for name in tag_providers {
        match component_problem(name) {
            Some(why) => {
                skipped.insert(
                    format!("ignition/tag-provider/{name}"),
                    refused(&format!("the tag provider name {why}")),
                );
            }
            None => {
                providers.insert(name.clone());
            }
        }
    }

    let mut yaml = String::from("schema_version: 1\n");
    if types.is_empty() {
        yaml.push_str("types: []\n");
    } else {
        yaml.push_str("types:\n");
        for t in &types {
            yaml.push_str(&format!("  - {}\n", yaml_scalar(t)));
        }
    }
    if providers.is_empty() {
        yaml.push_str("tag_providers: []\n");
    } else {
        yaml.push_str("tag_providers:\n");
        for p in &providers {
            yaml.push_str(&format!("  - {}\n", yaml_scalar(p)));
        }
    }
    let skipped = skipped
        .into_iter()
        .map(|(path, reason)| Skipped { path, reason })
        .collect();
    (yaml, skipped)
}

/// The live files of one unit, per collection.
#[derive(Default)]
struct Unit {
    core: Vec<RelPath>,
    mode: Vec<RelPath>,
}

/// Every source file of a unit (gateway-relative path and live bytes).
type Placed<'a> = Vec<(RelPath, &'a [u8])>;

fn place<'a>(unit: &Unit, env: &str, observed: &'a ObservedFiles) -> Result<Placed<'a>, String> {
    let mut placed = Placed::new();
    for path in &unit.core {
        placed.push((repo_path(path)?, observed.core[path].bytes.as_slice()));
    }
    for path in &unit.mode {
        let base = repo_path(path)?;
        placed.push((
            RelPath::new(&format!("modes/{env}/{base}"))?,
            observed.mode[path].bytes.as_slice(),
        ));
    }
    Ok(placed)
}

/// Repo paths (`gateways/<g>/...`) under or at `path`, as unit keys.
fn units_at<'a>(index: &'a BTreeMap<String, &'a str>, path: &str) -> BTreeSet<&'a str> {
    let trimmed = path.trim_end_matches('/');
    let mut found = BTreeSet::new();
    if let Some(unit) = index.get(trimmed) {
        found.insert(*unit);
    }
    let prefix = format!("{trimmed}/");
    for (file, unit) in index.range(prefix.clone()..) {
        if !file.starts_with(&prefix) {
            break;
        }
        found.insert(*unit);
    }
    found
}

/// The unit a secret location (`<file>: <json path> (...)`) belongs to. The
/// file part may itself contain `: `, so every split point is tried.
fn unit_of_secret<'a>(index: &'a BTreeMap<String, &'a str>, location: &str) -> Option<&'a str> {
    location
        .match_indices(": ")
        .find_map(|(at, _)| index.get(&location[..at]).copied())
}

/// Run the pull: assemble, render to a fixpoint, keep what round-trips.
pub fn pull(input: &PullInput<'_>) -> Result<PullOutcome, CoreError> {
    let observed = input.observed;
    let mut units: BTreeMap<String, Unit> = BTreeMap::new();
    for path in observed.core.keys() {
        units
            .entry(unit_key(path, &observed.core))
            .or_default()
            .core
            .push(path.clone());
    }
    for path in observed.mode.keys() {
        units
            .entry(unit_key(path, &observed.mode))
            .or_default()
            .mode
            .push(path.clone());
    }

    let prefix = format!("gateways/{}/", input.gateway);
    let mut skipped: BTreeMap<String, String> = BTreeMap::new();
    let mut placed: BTreeMap<&str, Placed<'_>> = BTreeMap::new();
    let mut index: BTreeMap<String, &str> = BTreeMap::new();
    for (key, unit) in &units {
        match place(unit, input.env, observed) {
            Ok(files) => {
                for (path, _) in &files {
                    index.insert(format!("{prefix}{path}"), key.as_str());
                }
                placed.insert(key.as_str(), files);
            }
            Err(why) => {
                skipped.insert(key.clone(), refused(&why));
            }
        }
    }

    for _round in 0..PULL_MAX_ROUNDS {
        let mut files = BTreeMap::new();
        for (key, unit_files) in &placed {
            if skipped.contains_key(*key) {
                continue;
            }
            for (path, bytes) in unit_files {
                files.insert(path.clone(), bytes.to_vec());
            }
        }
        let source = SourceTree { files };

        let result = render(&RenderInput {
            gateway: input.gateway,
            env: input.env,
            manifest: input.manifest,
            target: input.target,
            source: &source,
        });

        let mut dropped: BTreeMap<String, String> = BTreeMap::new();
        match result {
            Ok(rendered) => {
                let mismatches = mismatched_units(&units, &placed, &skipped, observed, &rendered)?;
                if mismatches.is_empty() {
                    return Ok(PullOutcome {
                        source,
                        rendered,
                        skipped: skipped
                            .into_iter()
                            .map(|(path, reason)| Skipped { path, reason })
                            .collect(),
                    });
                }
                for key in mismatches {
                    dropped.insert(key, REASON_NOT_ROUND_TRIPPABLE.to_string());
                }
            }
            Err(err) => {
                let (secrets, findings): (&[String], &[DeployFinding]) = match &err {
                    CoreError::EmbeddedSecret(d) => (&d.paths, &d.other_findings),
                    CoreError::DeployTreeInvalid(d) => (&[], &d.findings),
                    _ => return Err(err),
                };
                let mut unmapped = false;
                for location in secrets {
                    match unit_of_secret(&index, location) {
                        Some(key) => {
                            dropped.insert(key.to_string(), REASON_EMBEDDED_SECRET.to_string());
                        }
                        None => unmapped = true,
                    }
                }
                for finding in findings {
                    let hit = units_at(&index, &finding.file);
                    if hit.is_empty() {
                        unmapped = true;
                    }
                    for key in hit {
                        dropped
                            .entry(key.to_string())
                            .or_insert_with(|| refused(&finding.message));
                    }
                }
                // a finding about no unit (the manifest, the target) is the
                // operator's to fix; a round that drops nothing cannot progress
                if unmapped || dropped.is_empty() {
                    return Err(err);
                }
            }
        }
        skipped.extend(dropped);
    }
    Err(CoreError::deploy_tree_invalid(
        format!("gateways/{}", input.gateway),
        None,
        None,
        "pull did not converge",
    ))
}

/// The kept units whose rendered files are not exactly their effective live
/// files. A rendered group that belongs to no kept unit cannot be attributed
/// and is an error.
fn mismatched_units(
    units: &BTreeMap<String, Unit>,
    placed: &BTreeMap<&str, Placed<'_>>,
    skipped: &BTreeMap<String, String>,
    observed: &ObservedFiles,
    rendered: &RenderOutcome,
) -> Result<Vec<String>, CoreError> {
    let mut groups: BTreeMap<String, BTreeMap<&RelPath, &Vec<u8>>> = BTreeMap::new();
    for (path, bytes) in &rendered.tree.files {
        groups
            .entry(unit_key(path, &rendered.tree.files))
            .or_default()
            .insert(path, bytes);
    }
    let mut mismatched = Vec::new();
    let mut kept: BTreeSet<&str> = BTreeSet::new();
    for (key, unit) in units {
        if skipped.contains_key(key) || !placed.contains_key(key.as_str()) {
            continue;
        }
        kept.insert(key.as_str());
        let (paths, layer) = if unit.mode.is_empty() {
            (&unit.core, &observed.core)
        } else {
            (&unit.mode, &observed.mode)
        };
        let same = groups.get(key).is_some_and(|want| {
            want.len() == paths.len()
                && paths
                    .iter()
                    .all(|p| want.get(p).is_some_and(|b| b.as_slice() == layer[p].bytes))
        });
        if !same {
            mismatched.push(key.clone());
        }
    }
    if mismatched.is_empty()
        && let Some(stray) = groups.keys().find(|k| !kept.contains(k.as_str()))
    {
        return Err(CoreError::deploy_tree_invalid(
            "gateways",
            None,
            None,
            format!("pull could not attribute the rendered unit `{stray}` to a live unit"),
        ));
    }
    Ok(mismatched)
}
