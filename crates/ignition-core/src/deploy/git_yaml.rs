//! The git module's `git.yaml`, read for the keys ign needs (Phase 27, 27-17).
//!
//! `<data_dir>/git.yaml` is the module's settings file: a YAML list of maps,
//! one per project, keyed by `ignition_projectName` (module v2.3.4,
//! `GitCommissioningUtils.parseYaml`). It also holds a credential
//! (`user_password`, and a `repo_uri` may carry userinfo). The bytes are
//! untrusted data from a gateway host, so:
//!
//! - the size cap is checked before any parse;
//! - only an allow-list of keys is read: [`GitYamlEntry`] has no field for a
//!   credential and does not reject unknown keys, so serde skips every other
//!   key and its value is never held;
//! - an error never echoes a value of the file: it names `git.yaml` and, at
//!   most, a line number;
//! - `repo_uri` is reduced to its redacted form and its [`url_key`]; the raw
//!   text is dropped as the entry is built;
//! - `Debug` is written by hand and prints keys and booleans only.
//!
//! The module reads a boolean with `Boolean.parseBoolean`, so a YAML boolean
//! and the strings `true` / `false` (any case) both read.

use std::fmt;

use serde::Deserialize;
use serde::de::IgnoredAny;

use super::git_state::{redact_url, url_key};
use super::yamlio::YAML_MAX_BYTES;
use crate::error::CoreError;

/// A scalar that may be a YAML string, integer or boolean, anything else
/// ignored. `git.yaml` is written by hand and by a renderer; an entry of
/// another project that holds an odd value must not stop this project.
#[derive(Deserialize)]
#[serde(untagged)]
enum Scalar {
    Bool(bool),
    Int(i64),
    Str(String),
    Other(IgnoredAny),
}

impl Scalar {
    fn text(self) -> Option<String> {
        match self {
            Scalar::Str(s) => Some(s),
            Scalar::Int(n) => Some(n.to_string()),
            Scalar::Bool(b) => Some(b.to_string()),
            Scalar::Other(_) => None,
        }
    }

    fn truth(self) -> Option<bool> {
        match self {
            Scalar::Bool(b) => Some(b),
            Scalar::Str(s) => Some(s.trim().eq_ignore_ascii_case("true")),
            Scalar::Int(_) | Scalar::Other(_) => None,
        }
    }
}

/// The allow-listed keys of one entry as serde reads them. No `Debug`: this
/// holds the raw `repo_uri`, which may carry userinfo.
#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "ignition_projectName", default)]
    project: Option<Scalar>,
    #[serde(rename = "repo_uri", default)]
    repo_uri: Option<Scalar>,
    #[serde(rename = "repo_branch", default)]
    repo_branch: Option<Scalar>,
    #[serde(rename = "commissioning_importTags", default)]
    import_tags: Option<Scalar>,
    #[serde(rename = "commissioning_importThemes", default)]
    import_themes: Option<Scalar>,
    #[serde(rename = "commissioning_importImages", default)]
    import_images: Option<Scalar>,
    #[serde(rename = "commissioning_enforceBranch", default)]
    enforce_branch: Option<Scalar>,
    #[serde(rename = "production_mode", default)]
    production_mode: Option<Scalar>,
    #[serde(rename = "production_branch", default)]
    production_branch: Option<Scalar>,
    #[serde(rename = "production_tagPattern", default)]
    production_tag_pattern: Option<Scalar>,
    #[serde(rename = "tags_importOnStartup", default)]
    tags_import_on_startup: Option<Scalar>,
}

/// One project's entry of `git.yaml`: the allow-listed keys only.
///
/// There is no field for a credential, and the parser does not reject unknown
/// keys, so a credential value is skipped and never stored.
#[derive(Clone, PartialEq, Eq)]
pub struct GitYamlEntry {
    /// `ignition_projectName`.
    pub project: Option<String>,
    /// `repo_uri` with its userinfo removed.
    pub repo_url_redacted: Option<String>,
    /// The normalised key of `repo_uri` ([`url_key`]).
    pub repo_url_key: Option<String>,
    /// `repo_branch`: the branch the module tracks for the project.
    pub repo_branch: Option<String>,
    /// `commissioning_importTags`.
    pub import_tags: Option<bool>,
    /// `commissioning_importThemes`.
    pub import_themes: Option<bool>,
    /// `commissioning_importImages`.
    pub import_images: Option<bool>,
    /// `commissioning_enforceBranch`.
    pub enforce_branch: Option<bool>,
    /// `production_mode`.
    pub production_mode: Option<bool>,
    /// `production_branch`.
    pub production_branch: Option<String>,
    /// `production_tagPattern`.
    pub production_tag_pattern: Option<String>,
    /// `tags_importOnStartup`.
    pub tags_import_on_startup: Option<bool>,
}

impl From<RawEntry> for GitYamlEntry {
    fn from(raw: RawEntry) -> GitYamlEntry {
        let uri = raw.repo_uri.and_then(Scalar::text);
        GitYamlEntry {
            project: raw.project.and_then(Scalar::text),
            repo_url_redacted: uri.as_deref().map(redact_url),
            repo_url_key: uri.as_deref().map(url_key),
            repo_branch: raw.repo_branch.and_then(Scalar::text),
            import_tags: raw.import_tags.and_then(Scalar::truth),
            import_themes: raw.import_themes.and_then(Scalar::truth),
            import_images: raw.import_images.and_then(Scalar::truth),
            enforce_branch: raw.enforce_branch.and_then(Scalar::truth),
            production_mode: raw.production_mode.and_then(Scalar::truth),
            production_branch: raw.production_branch.and_then(Scalar::text),
            production_tag_pattern: raw.production_tag_pattern.and_then(Scalar::text),
            tags_import_on_startup: raw.tags_import_on_startup.and_then(Scalar::truth),
        }
    }
}

impl fmt::Debug for GitYamlEntry {
    /// Keys, booleans and the redacted repository key only; never a URL with
    /// userinfo and never a value the allow-list does not hold.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitYamlEntry")
            .field("project", &self.project)
            .field("repo_url_key", &self.repo_url_key)
            .field("repo_branch", &self.repo_branch)
            .field("import_tags", &self.import_tags)
            .field("import_themes", &self.import_themes)
            .field("import_images", &self.import_images)
            .field("enforce_branch", &self.enforce_branch)
            .field("production_mode", &self.production_mode)
            .field("production_branch", &self.production_branch)
            .field("tags_import_on_startup", &self.tags_import_on_startup)
            .finish_non_exhaustive()
    }
}

/// The refusal of a `git.yaml` that cannot be read: `deploy_run_invalid`
/// naming `git.yaml` and why, never a value from the file.
fn invalid(problem: impl fmt::Display) -> CoreError {
    CoreError::DeployRunInvalid {
        run_id: "unknown".to_string(),
        problem: format!("git.yaml: {problem}"),
    }
}

/// Parse `bytes` as the module's `git.yaml`: a YAML list of maps. An empty
/// file is an empty list. Refuses (`deploy_run_invalid`, naming `git.yaml`,
/// echoing nothing from the file) bytes over [`YAML_MAX_BYTES`], text that is
/// not UTF-8, YAML that does not parse and a top-level value that is not a
/// list of maps.
pub fn parse_git_yaml(bytes: &[u8]) -> Result<Vec<GitYamlEntry>, CoreError> {
    if bytes.len() > YAML_MAX_BYTES {
        return Err(invalid(format!(
            "is {} bytes, over the {YAML_MAX_BYTES} byte cap",
            bytes.len()
        )));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("is not valid UTF-8"))?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Check the shape first, so a map or a scalar at the top level gets a
    // plain message and an error of the typed parse names a line only.
    let top: yaml_serde::Value = yaml_serde::from_str(text).map_err(|e| match e.location() {
        Some(at) => invalid(format!("is not valid YAML (line {})", at.line())),
        None => invalid("is not valid YAML"),
    })?;
    match top {
        yaml_serde::Value::Null => return Ok(Vec::new()),
        yaml_serde::Value::Sequence(_) => {}
        _ => return Err(invalid("is not a list of project entries")),
    }
    let raw: Vec<RawEntry> = yaml_serde::from_str(text).map_err(|e| match e.location() {
        Some(at) => invalid(format!(
            "has an entry that is not a map of settings (line {})",
            at.line()
        )),
        None => invalid("has an entry that is not a map of settings"),
    })?;
    Ok(raw.into_iter().map(GitYamlEntry::from).collect())
}

/// The entry of `project` (`ignition_projectName`, an exact match). When the
/// file names the project twice and the entries disagree on `repo_branch`,
/// the file does not say which branch is meant, so there is no entry: the
/// caller falls through to its next branch source.
pub fn entry_for<'a>(entries: &'a [GitYamlEntry], project: &str) -> Option<&'a GitYamlEntry> {
    let mut matches = entries
        .iter()
        .filter(|e| e.project.as_deref() == Some(project));
    let first = matches.next()?;
    if matches.all(|e| e.repo_branch == first.repo_branch) {
        Some(first)
    } else {
        None
    }
}

/// A module start-sync action the kept `.git` re-arms (Phase 27, 27-18,
/// D-21): the words `--check` reports in `data.git.nodes[].hazards` and a
/// refusal names as its keys. Words only: a hazard never carries a value of
/// `git.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hazard {
    /// `commissioning_importImages: true`.
    ImportImages,
    /// `commissioning_importTags: true` or `tags_importOnStartup: true`.
    ImportTags,
    /// `commissioning_importThemes: true`.
    ImportThemes,
    /// `commissioning_enforceBranch: true`.
    EnforceBranch,
}

impl Hazard {
    /// The published word.
    pub fn word(self) -> &'static str {
        match self {
            Hazard::ImportImages => "import_images",
            Hazard::ImportTags => "import_tags",
            Hazard::ImportThemes => "import_themes",
            Hazard::EnforceBranch => "enforce_branch",
        }
    }

    /// What the module does at its next start with this flag set, as one
    /// clause that follows "the git module will ...".
    pub fn effect(self) -> &'static str {
        match self {
            Hazard::ImportImages => {
                "import the project's images/ directory (it warns that it replaces the gateway's \
                 images)"
            }
            Hazard::ImportTags => {
                "import tag providers from the project's tags/ directory over the gateway's"
            }
            Hazard::ImportThemes => "import the project's themes/ directory",
            Hazard::EnforceBranch => {
                "stash uncommitted work, switch the repository to the tracked branch and pull"
            }
        }
    }
}

/// The `git.yaml` flags of one entry that are true, in [`Hazard`] order. Both
/// `commissioning_importTags` and `tags_importOnStartup` read as
/// [`Hazard::ImportTags`].
pub fn hazards(entry: &GitYamlEntry) -> Vec<Hazard> {
    let mut found = Vec::new();
    if entry.import_images == Some(true) {
        found.push(Hazard::ImportImages);
    }
    if entry.import_tags == Some(true) || entry.tags_import_on_startup == Some(true) {
        found.push(Hazard::ImportTags);
    }
    if entry.import_themes == Some(true) {
        found.push(Hazard::ImportThemes);
    }
    if entry.enforce_branch == Some(true) {
        found.push(Hazard::EnforceBranch);
    }
    found
}

/// The hazards of every entry that names `project` (an exact match), once
/// each. A file that names the project twice is read as the union: the file
/// does not say which entry the module applies, so none is assumed harmless.
pub fn hazards_for(entries: &[GitYamlEntry], project: &str) -> Vec<Hazard> {
    let mut found: Vec<Hazard> = entries
        .iter()
        .filter(|e| e.project.as_deref() == Some(project))
        .flat_map(hazards)
        .collect();
    found.sort();
    found.dedup();
    found
}

/// What G3 observed a hazard to do at a gateway start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Damage {
    /// Damage whatever the node holds: the run refuses unless acknowledged.
    Refuse,
    /// Damage only when `tags/` is present on the node (D-G3): the next start
    /// would overwrite tag providers a Phase 26 owned-tag deploy manages.
    RefuseWithTags,
    /// No damage observed: reported as a warning only.
    Advisory,
}

/// Which hazards refuse, filled from the G3 and G6 captures (27-LIVE-CAPTURES,
/// `## G3 sync hazards`, `## G6 branch and remote shapes`, and the user
/// decision D-G3, 2026-11-13). Synthetic rig observations only: Ignition
/// 8.3.6 with the module's start sync, one flag true at a time. The quoted
/// effect lines are the `G3-EFFECT` lines of that capture.
pub const G3_DAMAGE: &[(Hazard, Damage)] = &[
    // `importImages images absent tags absent -> images unchanged (count=2 ...)`
    // and `importImages images present tags present -> images count=2 ... ->
    // count=3 ...`: the module logged that it deletes all gateway images, but
    // its delete loop found no legacy ImageRecord rows, so nothing was lost
    // and a present images/ only added its image. Advisory (D-G3); a gateway
    // upgraded from an older version was not tested.
    (Hazard::ImportImages, Damage::Advisory),
    // `importTags images present tags present -> ... tag exists=False ->
    // exists=True` and `tags_importOnStartup images present tags present ->
    // ... tag exists=False -> exists=True`: with tags/ present the start
    // imports it into the gateway's provider. With tags/ absent both lines
    // read `tag exists=False; project content unchanged` (nothing changed).
    // D-G3: refuse only when tags/ is present on the node.
    (Hazard::ImportTags, Damage::RefuseWithTags),
    // `importThemes images absent tags absent -> ... project content
    // unchanged; .git unchanged` and the same with images and tags present:
    // the import logged and changed nothing observable. No themes/ directory
    // was present in either variant, so a present one was not tried.
    // Advisory.
    (Hazard::ImportThemes, Damage::Advisory),
    // G6: `commissioning_enforceBranch: true at a gateway start stashes the
    // dirty working tree (a stash entry remains, refs/stash), switches the
    // branch and pulls (HEAD moved to the remote tip)`. D-G3: a hazard.
    (Hazard::EnforceBranch, Damage::Refuse),
];

/// The damage G3 recorded for `hazard`.
pub fn damage_of(hazard: Hazard) -> Damage {
    G3_DAMAGE
        .iter()
        .find(|(h, _)| *h == hazard)
        .map_or(Damage::Refuse, |(_, damage)| *damage)
}

/// Whether `hazard` refuses the run, given whether `tags/` is present on the
/// node. A hazard with no G3 row refuses (fail closed).
pub fn refuses(hazard: Hazard, tags_present: bool) -> bool {
    match damage_of(hazard) {
        Damage::Refuse => true,
        Damage::RefuseWithTags => tags_present,
        Damage::Advisory => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "s3cr3t-dummy-credential";

    fn code(err: &CoreError) -> &'static str {
        err.code()
    }

    fn problem(err: &CoreError) -> String {
        match err {
            CoreError::DeployRunInvalid { problem, .. } => problem.clone(),
            other => panic!("expected deploy_run_invalid, got {other:?}"),
        }
    }

    #[test]
    fn git_yaml_allow_list() {
        let text = format!(
            "- ignition_projectName: CapApp\n  repo_uri: https://user:{SECRET}@git.example/org/capapp.git\n  \
             repo_branch: production\n  user_password: {SECRET}\n  user_name: someone\n  \
             unknown_key: {{nested: [1, 2]}}\n  production_mode: true\n  \
             commissioning_enforceBranch: false\n\
             - ignition_projectName: Other\n  repo_branch: main\n  user_password: {SECRET}\n"
        );
        let entries = parse_git_yaml(text.as_bytes()).expect("parses");
        assert_eq!(entries.len(), 2);
        let first = &entries[0];
        assert_eq!(first.project.as_deref(), Some("CapApp"));
        assert_eq!(first.repo_branch.as_deref(), Some("production"));
        assert_eq!(first.production_mode, Some(true));
        assert_eq!(first.enforce_branch, Some(false));
        assert_eq!(
            first.repo_url_redacted.as_deref(),
            Some("https://git.example/org/capapp.git")
        );
        assert_eq!(
            first.repo_url_key.as_deref(),
            Some("git.example/org/capapp")
        );
        assert_eq!(entries[1].repo_branch.as_deref(), Some("main"));
        let shown = format!("{entries:?}");
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(!shown.contains("user:"), "{shown}");
        assert!(!shown.contains("someone"), "{shown}");
    }

    #[test]
    fn git_yaml_string_booleans() {
        let entries = parse_git_yaml(
            b"- ignition_projectName: A\n  production_mode: \"true\"\n  tags_importOnStartup: true\n  \
              commissioning_importTags: \"False\"\n  commissioning_enforceBranch: \"TRUE\"\n",
        )
        .expect("parses");
        let e = &entries[0];
        assert_eq!(e.production_mode, Some(true));
        assert_eq!(e.tags_import_on_startup, Some(true));
        assert_eq!(e.import_tags, Some(false));
        assert_eq!(e.enforce_branch, Some(true));
        assert_eq!(e.import_themes, None);
    }

    #[test]
    fn git_yaml_odd_values_of_known_keys_do_not_stop_the_file() {
        let entries = parse_git_yaml(
            b"- ignition_projectName: A\n  repo_branch: [a, b]\n  production_mode: {x: 1}\n  \
              production_branch: 2024\n",
        )
        .expect("parses");
        assert_eq!(entries[0].repo_branch, None);
        assert_eq!(entries[0].production_mode, None);
        assert_eq!(entries[0].production_branch.as_deref(), Some("2024"));
    }

    #[test]
    fn git_yaml_not_a_list() {
        for bad in [
            format!("repo_branch: production\nuser_password: {SECRET}\n"),
            format!("just-a-scalar-{SECRET}\n"),
            format!("- {SECRET}\n"),
            format!("- ignition_projectName: A\n  user_password: [{SECRET}\n"),
        ] {
            let err = parse_git_yaml(bad.as_bytes()).expect_err("refused");
            assert_eq!(code(&err), "deploy_run_invalid");
            let text = problem(&err);
            assert!(text.contains("git.yaml"), "{text}");
            assert!(!text.contains(SECRET), "{text}");
            assert!(!err.to_string().contains(SECRET), "{err}");
        }
        let err = parse_git_yaml(&[0xff, 0xfe, b'x']).expect_err("not utf-8");
        assert_eq!(code(&err), "deploy_run_invalid");
    }

    #[test]
    fn git_yaml_empty_is_an_empty_list() {
        assert!(parse_git_yaml(b"").expect("empty").is_empty());
        assert!(parse_git_yaml(b"  \n").expect("blank").is_empty());
        assert!(parse_git_yaml(b"[]\n").expect("list").is_empty());
        assert!(
            parse_git_yaml(b"# only a comment\n")
                .expect("comment")
                .is_empty()
        );
    }

    #[test]
    fn git_yaml_over_cap() {
        let big = vec![b'#'; YAML_MAX_BYTES + 1];
        let err = parse_git_yaml(&big).expect_err("over the cap");
        assert_eq!(code(&err), "deploy_run_invalid");
        assert!(problem(&err).contains("git.yaml"));
        // At the cap the size is fine.
        assert!(parse_git_yaml(&vec![b'#'; YAML_MAX_BYTES]).is_ok());
    }

    #[test]
    fn git_yaml_entry_for_matches_exactly() {
        let entries = parse_git_yaml(
            b"- ignition_projectName: CapApp\n  repo_branch: production\n\
              - ignition_projectName: capapp\n  repo_branch: main\n",
        )
        .expect("parses");
        assert_eq!(
            entry_for(&entries, "CapApp").and_then(|e| e.repo_branch.as_deref()),
            Some("production")
        );
        assert_eq!(
            entry_for(&entries, "capapp").and_then(|e| e.repo_branch.as_deref()),
            Some("main")
        );
        assert!(entry_for(&entries, "Cap").is_none());
        assert!(entry_for(&[], "CapApp").is_none());
    }

    #[test]
    fn git_yaml_entry_for_refuses_to_guess_between_branches() {
        let entries = parse_git_yaml(
            b"- ignition_projectName: CapApp\n  repo_branch: production\n\
              - ignition_projectName: CapApp\n  repo_branch: main\n",
        )
        .expect("parses");
        assert!(entry_for(&entries, "CapApp").is_none());
        let same = parse_git_yaml(
            b"- ignition_projectName: CapApp\n  repo_branch: main\n\
              - ignition_projectName: CapApp\n  repo_branch: main\n",
        )
        .expect("parses");
        assert!(entry_for(&same, "CapApp").is_some());
    }

    fn entries(text: &str) -> Vec<GitYamlEntry> {
        parse_git_yaml(text.as_bytes()).expect("parses")
    }

    #[test]
    fn hazards_read_the_true_flags_only() {
        let all = entries(
            "- ignition_projectName: A\n  commissioning_importImages: true\n  \
             commissioning_importTags: false\n  tags_importOnStartup: true\n  \
             commissioning_importThemes: \"true\"\n  commissioning_enforceBranch: true\n",
        );
        assert_eq!(
            hazards(&all[0]),
            vec![
                Hazard::ImportImages,
                Hazard::ImportTags,
                Hazard::ImportThemes,
                Hazard::EnforceBranch
            ]
        );
        let none = entries(
            "- ignition_projectName: A\n  commissioning_importImages: false\n  \
             repo_branch: main\n",
        );
        assert!(hazards(&none[0]).is_empty());
    }

    #[test]
    fn hazard_words_are_the_published_ones() {
        let words: Vec<&str> = [
            Hazard::ImportImages,
            Hazard::ImportTags,
            Hazard::ImportThemes,
            Hazard::EnforceBranch,
        ]
        .iter()
        .map(|h| h.word())
        .collect();
        assert_eq!(
            words,
            [
                "import_images",
                "import_tags",
                "import_themes",
                "enforce_branch"
            ]
        );
    }

    #[test]
    fn hazards_for_ignores_other_projects_and_unions_duplicates() {
        let e = entries(
            "- ignition_projectName: Other\n  commissioning_enforceBranch: true\n\
             - ignition_projectName: A\n  commissioning_importTags: true\n\
             - ignition_projectName: A\n  commissioning_importImages: true\n",
        );
        assert_eq!(
            hazards_for(&e, "A"),
            vec![Hazard::ImportImages, Hazard::ImportTags]
        );
        assert_eq!(hazards_for(&e, "Other"), vec![Hazard::EnforceBranch]);
        assert!(hazards_for(&e, "Third").is_empty());
    }

    /// D-G3: tags refuse only with `tags/` on the node, images and themes
    /// never, enforceBranch always; every hazard has a G3 row.
    #[test]
    fn g3_damage_rules() {
        assert_eq!(G3_DAMAGE.len(), 4);
        assert!(!refuses(Hazard::ImportImages, true));
        assert!(!refuses(Hazard::ImportImages, false));
        assert!(refuses(Hazard::ImportTags, true));
        assert!(!refuses(Hazard::ImportTags, false));
        assert!(!refuses(Hazard::ImportThemes, true));
        assert!(refuses(Hazard::EnforceBranch, false));
        assert!(refuses(Hazard::EnforceBranch, true));
        for hazard in [
            Hazard::ImportImages,
            Hazard::ImportTags,
            Hazard::ImportThemes,
            Hazard::EnforceBranch,
        ] {
            assert!(!hazard.effect().is_empty());
        }
    }
}
