//! The host copy of a tag-owning `deploy projects` run (26-12, TAGDEP-10).
//!
//! Beside the release record of a project
//! (`ign-deploy/<gateway>/projects/<project>/`, the deploy account's home on
//! an `ssh` node, the data directory on a `local` one) a tag-owning run keeps:
//!
//! ```text
//! journal.json                   the run in flight (ProjectJournal)
//! pre-image/run.json             what the set holds (PreImageRun), written last
//! pre-image/project.zip          the project export, when the run imports it
//! pre-image/release-record.json  the project's release record, when it had one
//! pre-image/tags-<P>.json        each existing owned provider's export
//! pre-image/provider-<P>.json    each existing owned provider's resource record
//! pre-image/record-<P>.json      each owned provider's previous tag record
//! ```
//!
//! Only the latest run's pre-image is kept: the next run writes over it.
//! `run.json` is written after every other file of the set and names each
//! file with its sha256, so a set without a `run.json` naming the asked run,
//! or a file that does not hash as `run.json` says, is incomplete. Both files
//! here are parsed as strictly as the release and tag records: an unknown or
//! missing field, another gateway or project, another schema, an unsafe name
//! or path, an unsorted list or a repeated key is `deploy_run_invalid` naming
//! the location. Problem texts never echo a value the file holds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::apply::run_store::RunId;
use super::component_problem;
use super::node::{self, Node};
use super::projects::Release;
use super::tag_record::{node_path_problem, order_problem};
use crate::error::CoreError;

/// File name of the host journal of a project run.
pub const PROJECT_JOURNAL_FILE: &str = "journal.json";

/// The largest project journal accepted (64 KiB), checked before any parse.
pub const PROJECT_JOURNAL_MAX_BYTES: usize = 65_536;

/// The directory beside the release record that holds the latest run's
/// pre-image.
pub const PRE_IMAGE_DIR: &str = "pre-image";

/// The largest pre-image file accepted (512 MiB): the project export ceiling
/// (`actions::projects::IMPORT_MAX_BYTES`), which is also the largest file in
/// a set.
pub const PRE_IMAGE_MAX_BYTES: usize = 512 * 1024 * 1024;

/// The share of `pre-image/run.json` one owned provider may take (40 MiB,
/// five tag record caps; ultrareview B4): a node of the provider is listed
/// at most three times (planned, sent, collided; or removed and deleted),
/// each line a little longer than in the tag record.
pub const PRE_IMAGE_RUN_PROVIDER_BYTES: usize = 5 * crate::deploy::tag_record::TAG_RECORD_MAX_BYTES;

/// The largest `pre-image/run.json` accepted for a set listing `providers`
/// owned providers: [`PRE_IMAGE_RUN_PROVIDER_BYTES`] per provider (at least
/// one) plus 1 MiB for the rest of the file. A file over
/// [`PRE_IMAGE_MAX_BYTES`] is refused before it is parsed.
pub fn pre_image_run_max_bytes(providers: usize) -> usize {
    PRE_IMAGE_RUN_PROVIDER_BYTES
        .saturating_mul(providers.max(1))
        .saturating_add(1024 * 1024)
}

/// The only schema version this build reads and writes.
const SCHEMA_VERSION: u32 = 1;

/// One file of the pre-image set. The names are fixed; a provider name inside
/// one must also pass the host's tag-record name check.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreImageFile {
    /// `run.json`, the completeness marker.
    Run,
    /// `project.zip`, the project export.
    Project,
    /// `release-record.json`, the project's release record before the run
    /// (ultrareview U1): `--rollback-run` puts it back. Absent when the
    /// project had none.
    Release,
    /// `tags-<P>.json`, a provider's native export.
    Tags(String),
    /// `provider-<P>.json`, a provider's resource record.
    Provider(String),
    /// `record-<P>.json`, a provider's tag record before the run.
    Record(String),
    /// `sent-<P>.json`, the nodes the run's plan adds to an existing
    /// provider as the run sends them (26-REVIEW CR-01): a JSON object of
    /// provider-relative path to flattened node.
    Sent(String),
}

impl PreImageFile {
    /// The file name.
    pub fn file_name(&self) -> String {
        match self {
            PreImageFile::Run => "run.json".to_string(),
            PreImageFile::Project => "project.zip".to_string(),
            PreImageFile::Release => "release-record.json".to_string(),
            PreImageFile::Tags(p) => format!("tags-{p}.json"),
            PreImageFile::Provider(p) => format!("provider-{p}.json"),
            PreImageFile::Record(p) => format!("record-{p}.json"),
            PreImageFile::Sent(p) => format!("sent-{p}.json"),
        }
    }

    /// The file `name` names, or `None` for a name outside the fixed set. A
    /// provider part must be one safe path component without a control
    /// character.
    pub fn parse(name: &str) -> Option<PreImageFile> {
        match name {
            "run.json" => return Some(PreImageFile::Run),
            "project.zip" => return Some(PreImageFile::Project),
            "release-record.json" => return Some(PreImageFile::Release),
            _ => {}
        }
        let stem = name.strip_suffix(".json")?;
        let (kind, provider) = if let Some(p) = stem.strip_prefix("tags-") {
            (0, p)
        } else if let Some(p) = stem.strip_prefix("provider-") {
            (1, p)
        } else if let Some(p) = stem.strip_prefix("record-") {
            (2, p)
        } else {
            (3, stem.strip_prefix("sent-")?)
        };
        if component_problem(provider).is_some() || provider.chars().any(char::is_control) {
            return None;
        }
        let provider = provider.to_string();
        Some(match kind {
            0 => PreImageFile::Tags(provider),
            1 => PreImageFile::Provider(provider),
            2 => PreImageFile::Record(provider),
            _ => PreImageFile::Sent(provider),
        })
    }

    /// The provider a per-provider file names.
    pub fn provider(&self) -> Option<&str> {
        match self {
            PreImageFile::Tags(p)
            | PreImageFile::Provider(p)
            | PreImageFile::Record(p)
            | PreImageFile::Sent(p) => Some(p),
            PreImageFile::Run | PreImageFile::Project | PreImageFile::Release => None,
        }
    }
}

/// Where a run in flight stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    /// The run is writing to the gateway.
    Applying,
    /// The run's rollback, automatic or `--rollback-run`, is writing.
    RollingBack,
}

impl JournalState {
    /// The word the journal and `data.tags.journal` carry.
    pub fn as_str(self) -> &'static str {
        match self {
            JournalState::Applying => "applying",
            JournalState::RollingBack => "rolling_back",
        }
    }
}

/// The host journal of a tag-owning run (26-12): written after the complete
/// pre-image and before the first gateway write, cleared after a green run or
/// a verified rollback, left after `rollback_failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectJournal {
    /// Always 1.
    pub schema_version: u32,
    /// The target's gateway.
    pub gateway: String,
    /// The gateway project.
    pub project: String,
    /// The run the journal belongs to.
    pub run_id: String,
    /// Where the run stands.
    pub state: JournalState,
    /// When the run (or the rollback) started, UTC.
    pub started_at: String,
    /// The owned providers, byte-sorted and unique.
    pub providers: Vec<String>,
}

const JOURNAL_FIELDS: [&str; 7] = [
    "schema_version",
    "gateway",
    "project",
    "run_id",
    "state",
    "started_at",
    "providers",
];

/// What a provider the run created is proven by (26-11): the create's answer,
/// what was sent, and the signature read right after the run's own create.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatedProof {
    /// The create answered success; `false` when its answer was lost.
    pub confirmed: bool,
    /// The signature of the record read right after the create; `None`
    /// when nothing proves the provider is the run's.
    pub signature: Option<String>,
    /// What the run sent: `enabled`.
    pub enabled: bool,
    /// What the run sent: `description`.
    pub description: Option<String>,
    /// What the run sent: `config`.
    pub config: Value,
}

/// One owned provider as the run planned it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreImageProvider {
    /// The provider.
    pub name: String,
    /// The provider-settings action: `same`, `modify` or `create`.
    pub settings: String,
    /// The nodes the run's plan adds, provider-relative, byte-sorted.
    pub added: Vec<String>,
    /// The nodes the run's plan changes, provider-relative, byte-sorted.
    pub changed: Vec<String>,
    /// Set once the run created the provider (26-11 proof).
    pub created: Option<CreatedProof>,
    /// The nodes the run's plan removes (26-20, D-02), provider-relative,
    /// byte-sorted. Absent in a set an earlier release wrote: none.
    #[serde(default)]
    pub removed: Vec<String>,
    /// Every path a delete call of the run named, written before the call
    /// was sent (26-20), provider-relative, byte-sorted, each one of
    /// `removed`: what `--rollback-run` re-creates from the pre-image.
    #[serde(default)]
    pub deleted: Vec<String>,
    /// The nodes of every import batch the run sent, written before the
    /// batch was sent (26-REVIEW CR-01), provider-relative, byte-sorted,
    /// each one of `added` or `changed`: what `--rollback-run` counts as
    /// possibly applied. Absent in a set an earlier release wrote: every
    /// planned node counts, and no created node can be shown to be the
    /// run's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<Vec<String>>,
    /// The added paths whose import answered that the node already existed
    /// (26-REVIEW CR-01), byte-sorted, each one of `sent`: never deleted.
    #[serde(default)]
    pub collided: Vec<String>,
}

/// `pre-image/run.json`: the complete set of one run's pre-image (26-12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreImageRun {
    /// Always 1.
    pub schema_version: u32,
    /// The target's gateway.
    pub gateway: String,
    /// The gateway project.
    pub project: String,
    /// The run the set belongs to.
    pub run_id: String,
    /// The release the run deployed.
    pub release: String,
    /// When the set was written, UTC.
    pub written_at: String,
    /// The project action of the run: `create`, `import`, `record` or
    /// `skip`.
    pub project_action: String,
    /// Whether the run planned a tag-group commit.
    pub groups: bool,
    /// The owned providers, byte-sorted by name and unique.
    pub providers: Vec<PreImageProvider>,
    /// Every other file of the set, by name, with its sha256.
    pub files: BTreeMap<String, String>,
}

const RUN_FIELDS: [&str; 10] = [
    "schema_version",
    "gateway",
    "project",
    "run_id",
    "release",
    "written_at",
    "project_action",
    "groups",
    "providers",
    "files",
];

/// At most 64 printable characters of `text`, for a problem message.
fn shown(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(64).collect()
}

fn lower_hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn stamp_ok(text: &str) -> bool {
    !text.is_empty() && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_graphic())
}

/// The strict front half both parses share: size, UTF-8, no repeated key, a
/// top-level object holding exactly `fields`.
fn strict_object(
    bytes: &[u8],
    cap: usize,
    fields: &[&str],
    bad: &dyn Fn(String) -> CoreError,
) -> Result<(), CoreError> {
    if bytes.is_empty() {
        return Err(bad("the file is empty".to_string()));
    }
    if bytes.len() > cap {
        return Err(bad(format!(
            "the file is {} bytes, over the {cap} byte cap",
            bytes.len()
        )));
    }
    // The checks of `node::classify` (strict UTF-8, no BOM, bounded depth,
    // no repeated key) without its 16 MiB cap: `cap` bounds the file.
    let text = std::str::from_utf8(bytes).map_err(|e| {
        bad(format!(
            "not valid UTF-8 (valid up to byte {})",
            e.valid_up_to()
        ))
    })?;
    if text.starts_with('\u{feff}') {
        return Err(bad("starts with a byte-order mark".to_string()));
    }
    let root = node::parse(text).map_err(bad)?;
    let Node::Obj(members) = &root else {
        return Err(bad("the top level is not a JSON object".to_string()));
    };
    for (key, _) in members {
        if !fields.contains(&key.as_str()) {
            return Err(bad(format!("unknown field `{}`", shown(key))));
        }
    }
    for field in fields {
        if root.get(field).is_none() {
            return Err(bad(format!("missing field `{field}`")));
        }
    }
    Ok(())
}

/// The checks of the names both files carry.
fn common_checks(
    schema_version: u32,
    (gateway, project, run_id): (&str, &str, &str),
    (want_gateway, want_project): (&str, &str),
    bad: &dyn Fn(String) -> CoreError,
) -> Result<(), CoreError> {
    if schema_version != SCHEMA_VERSION {
        return Err(bad(format!(
            "schema_version is not {SCHEMA_VERSION}, the only version this build reads"
        )));
    }
    if gateway != want_gateway {
        return Err(bad(format!(
            "gateway names a different gateway than the target (`{}`)",
            shown(want_gateway)
        )));
    }
    if project != want_project {
        return Err(bad(format!(
            "project names a different project than this path (`{}`)",
            shown(want_project)
        )));
    }
    if RunId::parse(run_id).is_err() {
        return Err(bad(
            "run_id is not 1 to 128 characters of [A-Za-z0-9._-]".to_string()
        ));
    }
    Ok(())
}

fn provider_names_ok(
    names: &[&str],
    what: &str,
    bad: &dyn Fn(String) -> CoreError,
) -> Result<(), CoreError> {
    for (i, name) in names.iter().enumerate() {
        if component_problem(name).is_some() || name.chars().any(char::is_control) {
            return Err(bad(format!("{what}[{i}] is not one safe provider name")));
        }
    }
    if names
        .windows(2)
        .any(|pair| pair[0].as_bytes() >= pair[1].as_bytes())
    {
        return Err(bad(format!(
            "{what} is out of byte order or repeats an entry"
        )));
    }
    Ok(())
}

fn location_error(location: &str) -> impl Fn(String) -> CoreError + '_ {
    move |problem| CoreError::DeployRunInvalid {
        run_id: "unknown".to_string(),
        problem: format!("{location}: {problem}"),
    }
}

impl ProjectJournal {
    /// Parse and validate `bytes` read from `location` for `gateway` and
    /// `project`. Every refusal is [`CoreError::DeployRunInvalid`].
    pub fn parse(
        bytes: &[u8],
        location: &str,
        gateway: &str,
        project: &str,
    ) -> Result<ProjectJournal, CoreError> {
        let bad = location_error(location);
        strict_object(bytes, PROJECT_JOURNAL_MAX_BYTES, &JOURNAL_FIELDS, &bad)?;
        let journal: ProjectJournal = serde_json::from_slice(bytes).map_err(|e| {
            bad(format!(
                "a field has the wrong type (line {} column {})",
                e.line(),
                e.column()
            ))
        })?;
        common_checks(
            journal.schema_version,
            (&journal.gateway, &journal.project, &journal.run_id),
            (gateway, project),
            &bad,
        )?;
        if !stamp_ok(&journal.started_at) {
            return Err(bad(
                "started_at is not 1 to 64 printable ASCII characters".to_string()
            ));
        }
        let names: Vec<&str> = journal.providers.iter().map(String::as_str).collect();
        provider_names_ok(&names, "providers", &bad)?;
        Ok(journal)
    }

    /// Pretty JSON in field order plus one LF.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}

impl PreImageRun {
    /// Parse and validate `bytes` read from `location` for `gateway` and
    /// `project`. Every refusal is [`CoreError::DeployRunInvalid`].
    pub fn parse(
        bytes: &[u8],
        location: &str,
        gateway: &str,
        project: &str,
    ) -> Result<PreImageRun, CoreError> {
        let bad = location_error(location);
        strict_object(bytes, PRE_IMAGE_MAX_BYTES, &RUN_FIELDS, &bad)?;
        let run: PreImageRun = serde_json::from_slice(bytes).map_err(|e| {
            bad(format!(
                "a field has the wrong type (line {} column {})",
                e.line(),
                e.column()
            ))
        })?;
        let cap = pre_image_run_max_bytes(run.providers.len());
        if bytes.len() > cap {
            return Err(bad(format!(
                "the file is {} bytes, over the {cap} byte cap for {} provider(s)",
                bytes.len(),
                run.providers.len()
            )));
        }
        common_checks(
            run.schema_version,
            (&run.gateway, &run.project, &run.run_id),
            (gateway, project),
            &bad,
        )?;
        if Release::try_parse(&run.release).is_none() {
            return Err(bad("release is not a semver tag".to_string()));
        }
        if !stamp_ok(&run.written_at) {
            return Err(bad(
                "written_at is not 1 to 64 printable ASCII characters".to_string()
            ));
        }
        if !matches!(
            run.project_action.as_str(),
            "create" | "import" | "record" | "skip"
        ) {
            return Err(bad(
                "project_action is not create, import, record or skip".to_string()
            ));
        }
        let names: Vec<&str> = run.providers.iter().map(|p| p.name.as_str()).collect();
        provider_names_ok(&names, "providers", &bad)?;
        for (i, provider) in run.providers.iter().enumerate() {
            if !matches!(provider.settings.as_str(), "same" | "modify" | "create") {
                return Err(bad(format!(
                    "providers[{i}].settings is not same, modify or create"
                )));
            }
            let no_sent = Vec::new();
            let sent = provider.sent.as_ref().unwrap_or(&no_sent);
            for (list, what) in [
                (&provider.added, "added"),
                (&provider.changed, "changed"),
                (&provider.removed, "removed"),
                (&provider.deleted, "deleted"),
                (sent, "sent"),
                (&provider.collided, "collided"),
            ] {
                for (j, path) in list.iter().enumerate() {
                    if let Some(why) = node_path_problem(path) {
                        return Err(bad(format!(
                            "providers[{i}].{what}[{j}] is not a provider-relative node path: it {why}"
                        )));
                    }
                }
                if let Some(j) = order_problem(list) {
                    return Err(bad(format!(
                        "providers[{i}].{what}[{j}] is out of byte order or repeats an entry"
                    )));
                }
            }
            if let Some((j, path)) = provider
                .deleted
                .iter()
                .enumerate()
                .find(|(_, path)| provider.removed.binary_search(path).is_err())
            {
                return Err(bad(format!(
                    "providers[{i}].deleted[{j}] `{}` is not one of the run's removals: the set \
                     is stale or partial",
                    shown(path)
                )));
            }
            if let Some((j, path)) = sent.iter().enumerate().find(|(_, path)| {
                provider.added.binary_search(path).is_err()
                    && provider.changed.binary_search(path).is_err()
            }) {
                return Err(bad(format!(
                    "providers[{i}].sent[{j}] `{}` is not one of the run's planned nodes: the set \
                     is stale or partial",
                    shown(path)
                )));
            }
            if let Some((j, path)) = provider.collided.iter().enumerate().find(|(_, path)| {
                sent.binary_search(path).is_err() || provider.added.binary_search(path).is_err()
            }) {
                return Err(bad(format!(
                    "providers[{i}].collided[{j}] `{}` is not an added node the run sent: the set \
                     is stale or partial",
                    shown(path)
                )));
            }
            if let Some(created) = &provider.created
                && created
                    .signature
                    .as_deref()
                    .is_some_and(|s| s.is_empty() || s.chars().any(char::is_control))
            {
                return Err(bad(format!(
                    "providers[{i}].created.signature is empty or holds a control character"
                )));
            }
        }
        for (name, sha) in &run.files {
            let Some(file) = PreImageFile::parse(name) else {
                return Err(bad(format!(
                    "files names `{}`, which is not a pre-image file name",
                    shown(name)
                )));
            };
            if file == PreImageFile::Run {
                return Err(bad("files names run.json itself".to_string()));
            }
            if let Some(provider) = file.provider()
                && !names.contains(&provider)
            {
                return Err(bad(format!(
                    "files names `{}` for a provider the set does not list",
                    shown(name)
                )));
            }
            if !lower_hex64(sha) {
                return Err(bad(format!(
                    "files.`{}` is not 64 lowercase hex digits",
                    shown(name)
                )));
            }
        }
        Ok(run)
    }

    /// Pretty JSON in field order plus one LF.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The provider entry named `name`.
    pub fn provider(&self, name: &str) -> Option<&PreImageProvider> {
        self.providers.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOC: &str = "ign-deploy/gw/projects/CapApp/journal.json";

    fn journal() -> ProjectJournal {
        ProjectJournal {
            schema_version: 1,
            gateway: "gw".to_string(),
            project: "CapApp".to_string(),
            run_id: "20261109T101500Z-ab12".to_string(),
            state: JournalState::Applying,
            started_at: "2026-11-09T10:15:00Z".to_string(),
            providers: vec!["PlantA".to_string()],
        }
    }

    fn run() -> PreImageRun {
        PreImageRun {
            schema_version: 1,
            gateway: "gw".to_string(),
            project: "CapApp".to_string(),
            run_id: "20261109T101500Z-ab12".to_string(),
            release: "v1.0.0".to_string(),
            written_at: "2026-11-09T10:15:00Z".to_string(),
            project_action: "import".to_string(),
            groups: false,
            providers: vec![PreImageProvider {
                name: "PlantA".to_string(),
                settings: "same".to_string(),
                added: vec!["Line1/Speed".to_string()],
                changed: Vec::new(),
                created: None,
                removed: Vec::new(),
                deleted: Vec::new(),
                sent: None,
                collided: Vec::new(),
            }],
            files: BTreeMap::from([
                ("project.zip".to_string(), "a".repeat(64)),
                ("tags-PlantA.json".to_string(), "b".repeat(64)),
            ]),
        }
    }

    #[test]
    fn pre_image_names_are_the_fixed_set() {
        for (name, file) in [
            ("run.json", PreImageFile::Run),
            ("project.zip", PreImageFile::Project),
            ("release-record.json", PreImageFile::Release),
            ("tags-Plant A.json", PreImageFile::Tags("Plant A".into())),
            ("provider-P.json", PreImageFile::Provider("P".into())),
            ("record-P.json", PreImageFile::Record("P".into())),
        ] {
            assert_eq!(PreImageFile::parse(name), Some(file.clone()), "{name}");
            assert_eq!(file.file_name(), name);
        }
        for bad in [
            "journal.json",
            "release.json",
            "tags-.json",
            "tags-...json",
            "tags-a/b.json",
            "other-P.json",
            "tags-P.zip",
            "run.json.tmp",
        ] {
            assert_eq!(PreImageFile::parse(bad), None, "{bad}");
        }
        assert_eq!(
            PRE_IMAGE_MAX_BYTES,
            crate::actions::projects::IMPORT_MAX_BYTES
        );
    }

    #[test]
    fn journal_round_trips_and_is_strict() {
        let j = journal();
        assert_eq!(
            ProjectJournal::parse(&j.to_bytes(), LOC, "gw", "CapApp").expect("parses"),
            j
        );
        let mut rolling = j.clone();
        rolling.state = JournalState::RollingBack;
        assert!(
            String::from_utf8(rolling.to_bytes())
                .unwrap()
                .contains("rolling_back")
        );
        for (bytes, gw, project) in [
            (j.to_bytes(), "other", "CapApp"),
            (j.to_bytes(), "gw", "Other"),
            (b"{}".to_vec(), "gw", "CapApp"),
            (b"not json".to_vec(), "gw", "CapApp"),
            (vec![b' '; PROJECT_JOURNAL_MAX_BYTES + 1], "gw", "CapApp"),
            (
                br#"{"schema_version":1,"schema_version":1}"#.to_vec(),
                "gw",
                "CapApp",
            ),
        ] {
            let err = ProjectJournal::parse(&bytes, LOC, gw, project).expect_err("refused");
            assert_eq!(err.code(), "deploy_run_invalid", "{err}");
            assert!(err.to_string().contains(LOC), "{err}");
        }
        let mut extra: Value = serde_json::from_slice(&j.to_bytes()).unwrap();
        extra["secret"] = Value::from("x");
        let err = ProjectJournal::parse(&serde_json::to_vec(&extra).unwrap(), LOC, "gw", "CapApp")
            .expect_err("unknown field");
        assert!(!err.to_string().contains("\"x\""), "{err}");
        let mut bad_run = j.clone();
        bad_run.run_id = "../x".to_string();
        assert!(ProjectJournal::parse(&bad_run.to_bytes(), LOC, "gw", "CapApp").is_err());
    }

    /// Ultrareview B4: `run.json` lists each provider's planned paths and
    /// the sent ones again, so a run whose plan is near one tag record's
    /// size is well over 8 MiB; the cap is sized for the aggregate.
    #[test]
    fn a_run_json_over_one_record_cap_still_parses() {
        let mut r = run();
        let paths: Vec<String> = (0..300_000).map(|i| format!("Line1/Tag{i:07}")).collect();
        r.providers[0].added = paths.clone();
        r.providers[0].sent = Some(paths);
        let bytes = r.to_bytes();
        assert!(
            bytes.len() > crate::deploy::tag_record::TAG_RECORD_MAX_BYTES,
            "{}",
            bytes.len()
        );
        PreImageRun::parse(&bytes, LOC, "gw", "CapApp").expect("parses");
        assert_eq!(
            pre_image_run_max_bytes(3) - pre_image_run_max_bytes(2),
            PRE_IMAGE_RUN_PROVIDER_BYTES
        );
        assert_eq!(pre_image_run_max_bytes(0), pre_image_run_max_bytes(1));
    }

    #[test]
    fn run_round_trips_and_is_strict() {
        let r = run();
        assert_eq!(
            PreImageRun::parse(&r.to_bytes(), LOC, "gw", "CapApp").expect("parses"),
            r
        );
        let mut stray = r.clone();
        stray
            .files
            .insert("tags-Other.json".to_string(), "c".repeat(64));
        assert!(PreImageRun::parse(&stray.to_bytes(), LOC, "gw", "CapApp").is_err());
        let mut odd = r.clone();
        odd.files.insert("journal.json".to_string(), "c".repeat(64));
        assert!(PreImageRun::parse(&odd.to_bytes(), LOC, "gw", "CapApp").is_err());
        let mut unsorted = r.clone();
        unsorted.providers[0].added = vec!["b".into(), "a".into()];
        assert!(PreImageRun::parse(&unsorted.to_bytes(), LOC, "gw", "CapApp").is_err());
        let mut action = r;
        action.project_action = "delete".to_string();
        assert!(PreImageRun::parse(&action.to_bytes(), LOC, "gw", "CapApp").is_err());
    }

    /// 26-REVIEW CR-01: `sent` names planned nodes only, `collided` added
    /// nodes it sent only, and a set an earlier release wrote without them
    /// parses with `sent` absent; `sent-<P>.json` is a file of the set.
    #[test]
    fn pre_image_run_sent_paths_are_planned() {
        let mut r = run();
        r.providers[0].sent = Some(vec!["Line1/Speed".into()]);
        r.providers[0].collided = vec!["Line1/Speed".into()];
        r.files
            .insert("sent-PlantA.json".to_string(), "c".repeat(64));
        let parsed = PreImageRun::parse(&r.to_bytes(), LOC, "gw", "CapApp").expect("valid");
        assert_eq!(parsed, r);
        let mut stray = r.clone();
        stray.providers[0].sent = Some(vec!["Line1/Other".into()]);
        stray.providers[0].collided = Vec::new();
        let err = PreImageRun::parse(&stray.to_bytes(), LOC, "gw", "CapApp").expect_err("stray");
        assert!(err.to_string().contains("sent[0]"), "{err}");
        let mut unsent = r.clone();
        unsent.providers[0].sent = Some(Vec::new());
        let err = PreImageRun::parse(&unsent.to_bytes(), LOC, "gw", "CapApp").expect_err("unsent");
        assert!(err.to_string().contains("collided[0]"), "{err}");
        let mut old: Value = serde_json::from_slice(&run().to_bytes()).unwrap();
        old["providers"][0]
            .as_object_mut()
            .unwrap()
            .remove("collided");
        let parsed = PreImageRun::parse(&serde_json::to_vec(&old).unwrap(), LOC, "gw", "CapApp")
            .expect("an earlier set");
        assert_eq!(parsed.providers[0].sent, None);
        assert!(parsed.providers[0].collided.is_empty());
        assert_eq!(
            PreImageFile::parse("sent-PlantA.json"),
            Some(PreImageFile::Sent("PlantA".to_string()))
        );
        assert_eq!(
            PreImageFile::Sent("P".to_string()).file_name(),
            "sent-P.json"
        );
    }

    /// 26-20: `removed` and `deleted` are node paths in byte order, every
    /// deleted path is one of the removals, and a set an earlier release
    /// wrote without them parses with both empty.
    #[test]
    fn pre_image_run_deleted_paths_are_removals() {
        let mut r = run();
        r.providers[0].removed = vec!["Line1/A".into(), "Line1/B".into()];
        r.providers[0].deleted = vec!["Line1/A".into()];
        let parsed = PreImageRun::parse(&r.to_bytes(), LOC, "gw", "CapApp").expect("valid");
        assert_eq!(parsed.providers[0].deleted, ["Line1/A"]);
        let mut stray = r.clone();
        stray.providers[0].deleted = vec!["Line1/C".into()];
        let err = PreImageRun::parse(&stray.to_bytes(), LOC, "gw", "CapApp").expect_err("stray");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert!(err.to_string().contains("Line1/C"), "{err}");
        let mut unsorted = r.clone();
        unsorted.providers[0].removed = vec!["Line1/B".into(), "Line1/A".into()];
        assert!(PreImageRun::parse(&unsorted.to_bytes(), LOC, "gw", "CapApp").is_err());
        let mut old: serde_json::Value = serde_json::from_slice(&run().to_bytes()).expect("json");
        for provider in old["providers"].as_array_mut().expect("providers") {
            let object = provider.as_object_mut().expect("object");
            object.remove("removed");
            object.remove("deleted");
        }
        let parsed = PreImageRun::parse(
            &serde_json::to_vec(&old).expect("json"),
            LOC,
            "gw",
            "CapApp",
        )
        .expect("an earlier set");
        assert!(parsed.providers[0].removed.is_empty() && parsed.providers[0].deleted.is_empty());
    }
}
