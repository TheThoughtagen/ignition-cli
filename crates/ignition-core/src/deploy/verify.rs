//! Pure evaluation of `ign deploy verify` (22-10, DPLN-07, DPLN-08, DPLN-09).
//!
//! [`evaluate`] takes what the declared tree says ([`Expected`]) and what the
//! action read from the gateway ([`ObservedState`]) and returns a
//! [`VerifyReport`]. It does no I/O, reads no clock and prints nothing. The
//! six checks always appear, in this order:
//!
//! | # | id | passes when |
//! |---|----|-------------|
//! | 1 | `scan` | no config scan is running |
//! | 2 | `mode` | the normalised `deploymentMode` equals the target's mode |
//! | 3 | `read_back` | every declared non-tag resource is in the REST effective view with an equal `config` (CV2 table below) |
//! | 4 | `files` | every declared tag unit's effective live copy is byte-equal to the render |
//! | 5 | `loaded` | no gateway log line since the last scan says `will not be loaded` for a declared resource (CV1) |
//! | 6 | `probes` | every `verify.tag_probes[]` tag reads quality `Good...` |
//!
//! ## Value-free by construction
//!
//! A [`Finding`] carries resource keys, JSON paths, tag paths, quality codes,
//! log timestamps and logger names. A log message is held in a [`LogLine`]
//! for matching only and is never serialized; a config, a tag value and a
//! file byte never enter the report. `Debug` of the observed types withholds
//! the same content.
//!
//! ## The read-back table (`22-LIVE-CAPTURES.md`, Decision CV2)
//!
//! Equality is JSON value equality (key order ignored; `1` equals `1.0`).
//!
//! - **N1** members whose value is JSON `null` are dropped from the file value
//!   and from the REST value, at every depth, for every type, before
//!   comparing. A null member therefore equals an absent one on either side.
//!   Some 8.3.6 types omit null members from REST (`database-translator`) and
//!   others echo them (`ignition/cobranding`'s unset image members). A null on
//!   one side against a non-null value on the other still differs, because
//!   after the drop the key exists on one side only.
//! - **N2** `ignition/security-levels`: the file must be a name-keyed subset
//!   of the REST tree (the gateway merges its own levels in, and orders
//!   differ).
//! - **N3** a unit with no `config.json` (its `resource.json` has
//!   `files: []`, `ignition/translations`): the keys of `resource.json`
//!   `attributes` that the REST `config` also carries must be equal; REST-only
//!   keys (`terms`) are ignored.
//! - **skip** a REST item whose effective collection is `system` is
//!   gateway-provided, not file-backed; the types in [`NO_READ_ROUTE`] answer
//!   404 on every read route. Both are listed as warnings, never counted as
//!   checked and never a pass for that resource.
//!
//! A false red on a type the rig did not exercise is fixed by adding a row
//! here, not by loosening equality (Pitfall 6).
//!
//! ## Who judges a difference (quick 261031-bbs)
//!
//! The table decides whether a resource differs; a [`ReadBackJudge`] decides
//! whether that difference is red. [`evaluate`] is strict. After an apply,
//! [`evaluate_with`] and [`ReadBackJudge::Baseline`] judge against the REST
//! view read before the write ([`ReadBackBaseline`]): only what the run wrote
//! wrongly or disturbed is red, and a difference that was already there is a
//! warning: a [`Finding`] with `status: warn` plus a `warnings` line. Paths are
//! compared as uncapped sets, never through a finding's capped detail.
//!
//! ## `loaded` is best effort (CV1)
//!
//! The gateway logs `will not be loaded` once, shortly after the scan that
//! found the resource, and not for every type (a uuid-less
//! `database-translator` is silent), and the REST view still lists an
//! unloaded resource. A clean window therefore proves only that the last scan
//! did not warn. The check says so in its detail, answers `warn` (not
//! observed) while a scan is running, and `fail` with `logs_unavailable` when
//! the log source cannot be read or is truncated.
//!
//! ## The verdict is not an error
//!
//! [`VERIFY_FAILED_SLUG`] lives only in the result's `slug` key. It is not a
//! `CoreError` variant and never joins `CoreError::DEPLOY_SLUGS` or the exit
//! table: a red verdict is a successful envelope whose `data.ok` is false
//! (Decision A).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};

use super::layout::TAG_TYPES;
use super::manifest::Manifest;
use super::plan::{PlanStatus, unit_key};
use super::render::RenderOutcome;
use crate::client::scan::ScanStatus;

/// The verdict slug of a red verify. Only ever in `data.slug`.
pub const VERIFY_FAILED_SLUG: &str = "deploy_verify_failed";

/// How long probes are polled before the verdict (SF-8).
pub const PROBE_DEADLINE: Duration = Duration::from_secs(30);

/// Time between two probe polls.
pub const PROBE_INTERVAL: Duration = Duration::from_secs(1);

/// Milliseconds the log window opens before `lastScanTimestamp -
/// lastScanDuration` (CV1: the entries are stamped after the scan end, so a
/// full duration plus one second is early under either reading of the field).
pub const LOG_WINDOW_SLACK_MS: i64 = 1_000;

/// Window used in place of `lastScanDuration` when the gateway omits it.
pub const LOG_WINDOW_FALLBACK_MS: i64 = 30_000;

/// Milliseconds to wait after a scan completed before an empty log answer
/// means anything (CV1: the entry became readable 516 to 1450 ms late).
pub const LOG_SETTLE_MS: i64 = 5_000;

/// The substring of the gateway line (CV1: the REST route returns it verbatim).
pub const WILL_NOT_BE_LOADED: &str = "will not be loaded";

/// Types whose list and singleton routes both answer 404 on 8.3.6 (CV2): they
/// cannot be read back and are reported, not compared.
pub const NO_READ_ROUTE: &[(&str, &str)] = &[
    ("com.inductiveautomation.opcua", "one-time"),
    ("ignition", "images"),
];

/// The collection name of gateway-provided, non-file-backed resources (CV2).
pub const SYSTEM_COLLECTION: &str = "system";

/// Most differing JSON paths one finding names.
const MAX_PATHS: usize = 20;

/// The path of a difference at the top of a value.
const ROOT_PATH: &str = "(root)";

/// Longest logger name or quality code copied into a finding.
const MAX_CODE_CHARS: usize = 96;

/// The six checks, in report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    /// No config scan is running.
    Scan,
    /// The gateway runs the target's mode.
    Mode,
    /// Declared resources read back equal through REST.
    ReadBack,
    /// Declared tag units are byte-equal on the host.
    Files,
    /// No `will not be loaded` line for a declared resource.
    Loaded,
    /// Tag probes read Good.
    Probes,
}

impl CheckId {
    /// The word the check serializes as (`read_back`, not `ReadBack`), for the
    /// human renderer.
    pub fn word(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Mode => "mode",
            Self::ReadBack => "read_back",
            Self::Files => "files",
            Self::Loaded => "loaded",
            Self::Probes => "probes",
        }
    }
}

/// The outcome of one check. `warn` never fails the verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Evaluated and good.
    Pass,
    /// Evaluated and bad, or not evaluable.
    Fail,
    /// Not a failure, but not a plain pass either.
    Warn,
}

impl CheckStatus {
    /// The word the status serializes as, for the human renderer.
    pub fn word(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Warn => "warn",
        }
    }
}

/// One value-free observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable code (`scan_active`, `mode_mismatch`, `resource_missing`,
    /// `resource_differs`, `files_differ`, `unreadable`, `logs_unavailable`,
    /// `unloaded_resource`, `tag_probes_absent`, `tag_probes_unavailable`,
    /// `tag_probe_not_good`).
    pub code: &'static str,
    /// What it is about: a resource key, unit key, tag path, `mode`, `logs`.
    pub subject: String,
    /// Paths, codes and timestamps only.
    pub detail: String,
    /// `Some(Warn)` only on a finding that does not count against its check
    /// (quick 261031-bbs); `None`, the default, counts, and is not
    /// serialized, so every envelope without a warn finding is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<CheckStatus>,
}

/// One check of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// Which check.
    pub id: CheckId,
    /// Its outcome.
    pub status: CheckStatus,
    /// One line of what was evaluated.
    pub detail: String,
    /// Findings, byte-sorted by subject (probes: manifest order).
    pub findings: Vec<Finding>,
}

/// The verify verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyReport {
    /// True when no check failed.
    pub ok: bool,
    /// [`VERIFY_FAILED_SLUG`] when `ok` is false, else null.
    pub slug: Option<&'static str>,
    /// All six checks in order.
    pub checks: Vec<Check>,
    /// Declared resources compared by `read_back`.
    pub resources_checked: usize,
    /// Non-failing notes.
    pub warnings: Vec<String>,
}

/// How one declared resource's `config.json` is compared.
#[derive(Clone, PartialEq)]
pub enum Compare {
    /// Equal after N1 (both sides).
    Exact(Value),
    /// N2: the file value is a name-keyed subset of the REST value.
    NameKeyedSubset(Value),
    /// N3: the `resource.json` attributes the REST config also carries.
    Attributes(Map<String, Value>),
}

impl fmt::Debug for Compare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Compare::Exact(_) => "Exact",
            Compare::NameKeyedSubset(_) => "NameKeyedSubset",
            Compare::Attributes(_) => "Attributes",
        };
        write!(f, "{kind}(<withheld>)")
    }
}

/// A declared non-tag resource that `read_back` compares.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedResource {
    /// `module/type/name`, or `module/type` for a singleton.
    pub key: String,
    /// Module id.
    pub module: String,
    /// Resource type.
    pub ty: String,
    /// Name; `None` for a singleton.
    pub name: Option<String>,
    /// What to compare.
    pub compare: Compare,
}

impl ExpectedResource {
    /// A resource compared by exact JSON equality (tests and callers).
    pub fn exact(module: &str, ty: &str, name: Option<&str>, config: Value) -> Self {
        Self::new(module, ty, name, Compare::Exact(config))
    }

    /// A resource with an explicit comparison.
    pub fn new(module: &str, ty: &str, name: Option<&str>, compare: Compare) -> Self {
        Self {
            key: resource_id(module, ty, name),
            module: module.to_string(),
            ty: ty.to_string(),
            name: name.map(str::to_string),
            compare,
        }
    }
}

/// `module/type/name`, or `module/type` for a singleton.
pub fn resource_id(module: &str, ty: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{module}/{ty}/{name}"),
        None => format!("{module}/{ty}"),
    }
}

/// What the declared tree says verify should find.
#[derive(Debug, Clone, PartialEq)]
pub struct Expected {
    /// The target's deployment mode.
    pub mode: String,
    /// Declared non-tag resources `read_back` compares, in key order.
    pub resources: Vec<ExpectedResource>,
    /// Declared resources `read_back` cannot compare, with the reason, in key
    /// order (no REST read route, or a `config.json` that is not JSON).
    pub not_checked: Vec<(String, String)>,
    /// Ids of every declared non-tag resource (compared or not), for the log
    /// match, in key order.
    pub declared_ids: Vec<String>,
    /// Unit keys of the declared tag units, in key order.
    pub tag_units: Vec<String>,
    /// `verify.tag_probes[]`, in manifest order.
    pub probes: Vec<String>,
}

impl Expected {
    /// Build from a render outcome and its manifest (deploy-layer types only).
    pub fn from_render(target_mode: &str, outcome: &RenderOutcome, manifest: &Manifest) -> Self {
        let files = &outcome.tree.files;
        let is_tag = |module: &str, ty: &str| module == "ignition" && TAG_TYPES.contains(&ty);

        let mut tag_units = BTreeSet::new();
        for path in files.keys() {
            let mut parts = path.components();
            if let (Some(module), Some(ty)) = (parts.next(), parts.next())
                && is_tag(module, ty)
            {
                tag_units.insert(unit_key(path, files));
            }
        }

        let mut resources = BTreeMap::new();
        let mut not_checked = BTreeMap::new();
        let mut declared_ids = BTreeSet::new();
        for res in &outcome.resources {
            if is_tag(&res.module, &res.ty) {
                continue;
            }
            let key = resource_id(&res.module, &res.ty, res.name.as_deref());
            declared_ids.insert(key.clone());
            if NO_READ_ROUTE
                .iter()
                .any(|(m, t)| *m == res.module && *t == res.ty)
            {
                not_checked.insert(key, "no REST read route".to_string());
                continue;
            }
            let file = |name: &str| {
                super::layout::RelPath::new(&format!("{key}/{name}"))
                    .ok()
                    .and_then(|p| files.get(&p))
                    .map(Vec::as_slice)
            };
            let compare = match compare_for(&res.module, &res.ty, file) {
                Ok(compare) => compare,
                Err(reason) => {
                    not_checked.insert(key, reason.to_string());
                    continue;
                }
            };
            resources.insert(
                key.clone(),
                ExpectedResource {
                    key,
                    module: res.module.clone(),
                    ty: res.ty.clone(),
                    name: res.name.clone(),
                    compare,
                },
            );
        }

        Expected {
            mode: target_mode.to_string(),
            resources: resources.into_values().collect(),
            not_checked: not_checked.into_iter().collect(),
            declared_ids: declared_ids.into_iter().collect(),
            tag_units: tag_units.into_iter().collect(),
            probes: manifest
                .verify
                .as_ref()
                .map(|v| v.tag_probes.clone())
                .unwrap_or_default(),
        }
    }
}

/// How `read_back` compares one declared resource of `module`/`ty`, from its
/// unit's files (`file` answers a file name such as `config.json`): its
/// `config.json` as JSON (a name-keyed subset for `ignition/security-levels`,
/// exact otherwise), else the `attributes` of its `resource.json`. `Err` is
/// the reason it cannot be compared. Shared by [`Expected::from_render`] (the
/// declared files) and the apply's live-file basis (quick 261031-bbs), so
/// both build the comparison the same way.
pub fn compare_for<'f>(
    module: &str,
    ty: &str,
    file: impl Fn(&str) -> Option<&'f [u8]>,
) -> Result<Compare, &'static str> {
    if let Some(bytes) = file("config.json") {
        match serde_json::from_slice::<Value>(bytes) {
            Ok(value) if module == "ignition" && ty == "security-levels" => {
                Ok(Compare::NameKeyedSubset(value))
            }
            Ok(value) => Ok(Compare::Exact(value)),
            Err(_) => Err("config.json is not JSON"),
        }
    } else if let Some(attributes) = file("resource.json")
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
        .and_then(|v| v.get("attributes").and_then(Value::as_object).cloned())
    {
        Ok(Compare::Attributes(attributes))
    } else {
        Err("no config.json to compare")
    }
}

/// What the gateway said about one declared resource.
#[derive(Clone, PartialEq)]
pub enum Remote {
    /// Not in the effective view.
    Missing,
    /// In the effective view with this `config`.
    Config(Value),
    /// In the effective view, but from the `system` collection.
    System,
}

impl fmt::Debug for Remote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Remote::Missing => f.write_str("Missing"),
            Remote::Config(_) => f.write_str("Config(<withheld>)"),
            Remote::System => f.write_str("System"),
        }
    }
}

/// One gateway log entry kept for matching.
#[derive(Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Epoch ms on the gateway clock.
    pub timestamp: i64,
    /// Logger name.
    pub logger: String,
    /// The message. Matched against, never serialized, never in `Debug`.
    pub message: String,
}

impl fmt::Debug for LogLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogLine")
            .field("timestamp", &self.timestamp)
            .field("logger", &self.logger)
            .field("message", &"<withheld>")
            .finish()
    }
}

/// One tag probe reading. The value is never carried.
#[derive(Clone, PartialEq, Eq)]
pub struct ProbeReading {
    /// The tag path read.
    pub path: String,
    /// The quality string (`Good`, `Bad_NotFound`, ...).
    pub quality: String,
}

impl fmt::Debug for ProbeReading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProbeReading")
            .field("path", &self.path)
            .field("quality", &quality_code(&self.quality))
            .finish()
    }
}

/// What the action read.
#[derive(Debug, Clone)]
pub struct ObservedState {
    /// The config scan status (gateway clock).
    pub scan: ScanStatus,
    /// The normalised active deployment mode.
    pub mode: Option<String>,
    /// Per declared, compared resource key.
    pub read_back: BTreeMap<String, Remote>,
    /// Plan status per declared tag unit key.
    pub files: Vec<(String, PlanStatus)>,
    /// The log lines since the last scan, or the slug of why they could not
    /// be read.
    pub logs: Result<Vec<LogLine>, String>,
    /// The probe readings, or the slug of why they could not be read; `None`
    /// when no probe read was made.
    pub probes: Option<Result<Vec<ProbeReading>, String>>,
    /// The units with a live path the host's sftp server cannot read
    /// (quick-261029-ms3): unit key and whether it is declared. Never
    /// treated as absent: a declared one is a red `unreadable` finding, an
    /// undeclared one a warning.
    pub unreadable: Vec<(String, bool)>,
}

/// The start of the log window for the last scan (CV1), epoch ms on the
/// gateway clock.
pub fn log_window_start(scan: &ScanStatus) -> i64 {
    let duration = scan.last_scan_duration.unwrap_or(LOG_WINDOW_FALLBACK_MS);
    (scan.last_scan_timestamp - duration - LOG_WINDOW_SLACK_MS).max(0)
}

/// How `read_back` judges a difference between a declared file and the REST
/// effective view (quick 261031-bbs).
#[derive(Debug, Clone, Copy)]
pub enum ReadBackJudge<'a> {
    /// Every difference is red (the CV2 table as is).
    Strict,
    /// Standalone `deploy verify`: a difference on a resource whose plan
    /// status is `same` is a warning.
    Standalone {
        /// Plan status per declared non-tag resource key.
        file_status: &'a BTreeMap<String, PlanStatus>,
    },
    /// After an apply: judged against the REST view read before the write.
    Baseline(&'a ReadBackBaseline),
}

/// What a resource's pre-apply REST view was measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineBasis {
    /// Not written by the run: its declared file (equal to the live one).
    DeclaredFile,
    /// Written (`changed`): the live file before the apply.
    LiveFile,
    /// Judged as [`ReadBackJudge::Strict`] does.
    Strict,
}

/// How the run writes a declared resource.
#[derive(Debug, Clone, PartialEq)]
pub enum Written {
    /// New: no live copy before the apply.
    Added,
    /// Changed: the comparison built from the pre-apply live file, when one
    /// could be built.
    Changed(Option<Compare>),
}

/// One resource of a [`ReadBackBaseline`].
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineEntry {
    basis: BaselineBasis,
    before: Remote,
    differs_at: BTreeSet<String>,
    writes_at: BTreeSet<String>,
}

impl BaselineEntry {
    /// What the pre-apply view was measured against.
    pub fn basis(&self) -> BaselineBasis {
        self.basis
    }

    /// The uncapped paths at which the pre-apply view differed from the
    /// basis.
    pub fn differs_at(&self) -> &BTreeSet<String> {
        &self.differs_at
    }

    /// The uncapped paths the run's write changes: where the desired file
    /// differs from the pre-apply live file ([`BaselineBasis::LiveFile`]
    /// only; empty otherwise).
    pub fn writes_at(&self) -> &BTreeSet<String> {
        &self.writes_at
    }
}

/// The REST effective view of every declared, compared resource of one
/// node, read before an apply writes anything. `Debug` withholds every value.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBackBaseline {
    node: String,
    resources: BTreeMap<String, BaselineEntry>,
}

impl ReadBackBaseline {
    /// The baseline of `node` from its pre-apply read `before`. A key in
    /// `written` is one the run writes. Per declared, compared resource:
    ///
    /// - not written: [`BaselineBasis::DeclaredFile`], its pre-apply REST
    ///   view measured against the declared file (a `same` unit's live file
    ///   is the declared file);
    /// - written `changed` with a comparison built from the pre-apply live
    ///   file, and a pre-apply REST `config`: [`BaselineBasis::LiveFile`],
    ///   measured against that live file. Measured against the desired file
    ///   instead, every path the apply changes would read as pre-existing, and
    ///   a write that never took effect would go green. The entry also keeps
    ///   the paths the write changes (the desired file against the live one),
    ///   so each written path can be judged on its own;
    /// - anything else (`added`, a `changed` unit with no live comparison, a
    ///   pre-apply view that is missing or `system`): [`BaselineBasis::Strict`].
    ///
    /// The path sets are uncapped: never derived from a finding's detail,
    /// which names at most 20.
    pub fn build(
        node: &str,
        expected: &Expected,
        before: &BTreeMap<String, Remote>,
        written: &BTreeMap<String, Written>,
    ) -> ReadBackBaseline {
        let against = |remote: &Remote, compare: &Compare| match remote {
            Remote::Config(rest) => read_back_paths(compare, rest),
            Remote::Missing | Remote::System => BTreeSet::new(),
        };
        let resources = expected
            .resources
            .iter()
            .map(|res| {
                let r0 = before.get(&res.key).cloned().unwrap_or(Remote::Missing);
                let (basis, differs_at, writes_at) = match (written.get(&res.key), &r0) {
                    (None, _) => (
                        BaselineBasis::DeclaredFile,
                        against(&r0, &res.compare),
                        BTreeSet::new(),
                    ),
                    (Some(Written::Changed(Some(live))), Remote::Config(rest)) => (
                        BaselineBasis::LiveFile,
                        read_back_paths(live, rest),
                        write_paths(&res.compare, live),
                    ),
                    (Some(_), _) => (
                        BaselineBasis::Strict,
                        against(&r0, &res.compare),
                        BTreeSet::new(),
                    ),
                };
                (
                    res.key.clone(),
                    BaselineEntry {
                        basis,
                        before: r0,
                        differs_at,
                        writes_at,
                    },
                )
            })
            .collect();
        ReadBackBaseline {
            node: node.to_string(),
            resources,
        }
    }

    /// The node it was read from.
    pub fn node(&self) -> &str {
        &self.node
    }

    /// The entry of `key`, when it was declared.
    pub fn entry(&self, key: &str) -> Option<&BaselineEntry> {
        self.resources.get(key)
    }

    /// The value-free record of this baseline, for `read_back_baseline.json`.
    /// Per resource: the state of its pre-apply view, a SHA-256 of that
    /// view's canonical form (`config` only: null members stripped, keys
    /// sorted at every depth, compact), the basis and the uncapped,
    /// byte-sorted paths. Never a value, and no per-path hash (R5).
    pub fn record(&self) -> BaselineRecord {
        let resources = self
            .resources
            .iter()
            .map(|(key, entry)| {
                let (state, rest_sha256) = match &entry.before {
                    Remote::Config(rest) => ("config", Some(canonical_sha256(rest))),
                    Remote::Missing => ("missing", None),
                    Remote::System => ("system", None),
                };
                (
                    key.clone(),
                    BaselineRecordEntry {
                        state,
                        rest_sha256,
                        basis: entry.basis,
                        differs_at: entry.differs_at.iter().cloned().collect(),
                    },
                )
            })
            .collect();
        BaselineRecord {
            node: self.node.clone(),
            resources,
        }
    }
}

/// The value-free record of one node's [`ReadBackBaseline`]
/// (`read_back_baseline.json`, quick 261031-bbs P5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaselineRecord {
    /// `master` or `backup`.
    pub node: String,
    /// Per declared, compared resource key.
    pub resources: BTreeMap<String, BaselineRecordEntry>,
}

/// One resource of a [`BaselineRecord`]: never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaselineRecordEntry {
    /// `config`, `missing` or `system`.
    pub state: &'static str,
    /// SHA-256 of the canonical, null-stripped pre-apply REST `config`
    /// (`config` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest_sha256: Option<String>,
    /// What the pre-apply view was measured against.
    pub basis: BaselineBasis,
    /// The uncapped, byte-sorted paths at which it differed.
    pub differs_at: Vec<String>,
}

/// The SHA-256 of `value`'s canonical form: null members stripped, object
/// keys sorted by an explicit walk at every depth (independent of how
/// `serde_json` orders a map), compact.
fn canonical_sha256(value: &Value) -> String {
    fn write(value: &Value, out: &mut String) {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                out.push('{');
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&Value::String(key.clone()).to_string());
                    out.push(':');
                    write(&map[key], out);
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(']');
            }
            scalar => out.push_str(&scalar.to_string()),
        }
    }
    let mut out = String::new();
    write(&strip_nulls(value), &mut out);
    super::tree::sha256_hex(out.as_bytes())
}

/// Whether `path` is covered by `by`: `by` holds `(root)`, the path itself,
/// or an ancestor of it (the next character after the ancestor is `.` or
/// `[`).
pub fn path_covered(path: &str, by: &BTreeSet<String>) -> bool {
    by.iter().any(|ancestor| path_under(path, ancestor))
}

/// Whether `path` is `ancestor`, lies under it (the next character after it
/// is `.` or `[`), or `ancestor` is `(root)`.
fn path_under(path: &str, ancestor: &str) -> bool {
    ancestor == ROOT_PATH
        || path == ancestor
        || (path.len() > ancestor.len()
            && path.starts_with(ancestor)
            && matches!(path.as_bytes()[ancestor.len()], b'.' | b'['))
}

/// Whether two paths name overlapping parts of a document: equal, or one is
/// an ancestor of the other (`(root)` overlaps everything).
fn paths_overlap(a: &str, b: &str) -> bool {
    path_under(a, b) || path_under(b, a)
}

/// Evaluate `observed` against `expected`.
pub fn evaluate(expected: &Expected, observed: &ObservedState) -> VerifyReport {
    evaluate_with(expected, observed, &ReadBackJudge::Strict)
}

/// [`evaluate`], with `read_back` judged by `judge`.
pub fn evaluate_with(
    expected: &Expected,
    observed: &ObservedState,
    judge: &ReadBackJudge<'_>,
) -> VerifyReport {
    let mut warnings = Vec::new();
    let (read_back, resources_checked) =
        read_back_check_with(expected, &observed.read_back, judge, &mut warnings);
    let checks = vec![
        check_scan(observed),
        check_mode(expected, observed),
        read_back,
        check_files(expected, observed, &mut warnings),
        check_loaded(expected, observed),
        check_probes(expected, observed),
    ];
    for check in &checks {
        if check.status == CheckStatus::Warn {
            warnings.push(format!("{}: {}", check_word(check.id), check.detail));
        }
    }
    let ok = checks.iter().all(|c| c.status != CheckStatus::Fail);
    VerifyReport {
        ok,
        slug: (!ok).then_some(VERIFY_FAILED_SLUG),
        checks,
        resources_checked,
        warnings,
    }
}

fn check_word(id: CheckId) -> &'static str {
    match id {
        CheckId::Scan => "scan",
        CheckId::Mode => "mode",
        CheckId::ReadBack => "read_back",
        CheckId::Files => "files",
        CheckId::Loaded => "loaded",
        CheckId::Probes => "probes",
    }
}

fn finding(code: &'static str, subject: impl Into<String>, detail: impl Into<String>) -> Finding {
    Finding {
        code,
        subject: subject.into(),
        detail: detail.into(),
        status: None,
    }
}

/// Sort findings by subject bytes, then code, so the report does not depend
/// on input order.
fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        a.subject
            .as_bytes()
            .cmp(b.subject.as_bytes())
            .then_with(|| a.code.cmp(b.code))
    });
}

fn finished(id: CheckId, detail: String, findings: Vec<Finding>) -> Check {
    Check {
        id,
        status: if findings.is_empty() {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail,
        findings,
    }
}

// ---- 1. scan ------------------------------------------------------------------

fn check_scan(observed: &ObservedState) -> Check {
    if observed.scan.scan_active {
        return finished(
            CheckId::Scan,
            "a config scan is running".to_string(),
            vec![finding(
                "scan_active",
                "config",
                "the gateway reports an active config scan; verify reads settled state only",
            )],
        );
    }
    finished(
        CheckId::Scan,
        "no config scan is running".to_string(),
        Vec::new(),
    )
}

// ---- 2. mode ------------------------------------------------------------------

fn check_mode(expected: &Expected, observed: &ObservedState) -> Check {
    mode_check(expected, observed.mode.as_deref())
}

/// The `mode` check over an already-read, normalised mode. The backup node's
/// check shares it with the master's (24-05).
pub(crate) fn mode_check(expected: &Expected, mode: Option<&str>) -> Check {
    if mode == Some(expected.mode.as_str()) {
        return finished(
            CheckId::Mode,
            format!("the gateway runs mode `{}`", expected.mode),
            Vec::new(),
        );
    }
    let seen = match mode {
        Some(mode) => format!("`{}`", printable(mode)),
        None => "no mode".to_string(),
    };
    finished(
        CheckId::Mode,
        "the gateway does not run the target's mode".to_string(),
        vec![finding(
            "mode_mismatch",
            "mode",
            format!("expected `{}`, the gateway reports {seen}", expected.mode),
        )],
    )
}

// ---- 3. read_back -------------------------------------------------------------

/// The `read_back` check judged by `judge` (quick 261031-bbs).
///
/// [`ReadBackJudge::Strict`] is the CV2 table: every difference and every
/// missing resource is red. [`ReadBackJudge::Standalone`] turns the
/// difference or absence of a resource whose plan status is `same` into a
/// warning (the live files equal the repo; only the gateway's REST view
/// differs); any other status, or none, stays red. [`ReadBackJudge::Baseline`]
/// judges each resource against the REST view read before the apply wrote
/// anything:
///
/// - not written ([`BaselineBasis::DeclaredFile`]): red only when its REST
///   view moved since the baseline AND still differs from its file, or when
///   it disappeared; a difference or absence that was already there is a
///   warning, and a move toward the file passes;
/// - written `changed` ([`BaselineBasis::LiveFile`]): red when the post-apply
///   difference holds a path not covered by the pre-apply difference from
///   the live file (an ancestor covers its descendants, `(root)` covers
///   everything); when the desired file differs from the live one but the
///   REST view did not move at all (the write did not take effect,
///   amendment A1); or, per written path, when the post-apply difference
///   still overlaps a path the write changes (that path, an ancestor or a
///   descendant) and the REST view did not move anywhere overlapping that
///   path (that part of the write did not take effect, CR-01); otherwise a
///   warning;
/// - everything else ([`BaselineBasis::Strict`], or no baseline entry): as
///   strict.
///
/// A warning is a finding with `status: Some(Warn)` plus one `warnings` line
/// naming the key and its paths; the check is `fail` when any finding has
/// no warn status, `warn` when only warn findings are left, `pass` otherwise.
///
/// Residual risks, accepted and documented: a written path whose REST view
/// moved during the apply, but to a value that still differs from the file in
/// a pre-existing way, is covered (REST moved there, so the write reached it);
/// a written path the gateway normalises back to the value it already showed
/// reads as not taken (a false red); an operator change, or a pending
/// unscanned edit, between the baseline and the post-scan read reads as this
/// run's disturbance (a false red that rolls back); an `added` unit is judged
/// strictly, so a hand-written minimal `config.json` the gateway fills with
/// defaults rolls back (`deploy pull` writes the gateway's own shape).
pub(crate) fn read_back_check_with(
    expected: &Expected,
    read_back: &BTreeMap<String, Remote>,
    judge: &ReadBackJudge<'_>,
    warnings: &mut Vec<String>,
) -> (Check, usize) {
    let mut findings = Vec::new();
    let mut checked = 0usize;
    let mut warned = 0usize;
    for res in &expected.resources {
        let after = read_back.get(&res.key).unwrap_or(&Remote::Missing);
        let paths = match after {
            Remote::System => {
                warnings.push(format!(
                    "read_back: {} not compared: the effective copy is in the `{SYSTEM_COLLECTION}` collection (gateway-provided, not file-backed)",
                    res.key
                ));
                continue;
            }
            Remote::Missing => None,
            Remote::Config(rest) => Some(read_back_paths(&res.compare, rest)),
        };
        checked += 1;
        match judge_resource(&res.key, after, paths.as_ref(), judge) {
            Verdict::Pass => {}
            Verdict::Red(f) => findings.push(f),
            Verdict::Warn(mut f, line) => {
                f.status = Some(CheckStatus::Warn);
                findings.push(f);
                warnings.push(line);
                warned += 1;
            }
        }
    }
    for (key, reason) in &expected.not_checked {
        warnings.push(format!("read_back: {key} not compared: {reason}"));
    }
    if checked == 0 && findings.is_empty() {
        warnings.push(
            "read_back: no declared non-tag resource was compared (nothing to read back)"
                .to_string(),
        );
    }
    sort_findings(&mut findings);
    let mut detail =
        format!("{checked} declared resource(s) compared with the REST effective view");
    let status = if findings.iter().any(|f| f.status.is_none()) {
        CheckStatus::Fail
    } else if findings.is_empty() {
        CheckStatus::Pass
    } else {
        detail.push_str(&format!("; {warned} difference(s) reported as warnings"));
        CheckStatus::Warn
    };
    (
        Check {
            id: CheckId::ReadBack,
            status,
            detail,
            findings,
        },
        checked,
    )
}

/// The `read_back` check over already-read resources, judged strictly. The
/// backup node's check shares it with the master's (24-05).
pub(crate) fn read_back_check(
    expected: &Expected,
    read_back: &BTreeMap<String, Remote>,
    warnings: &mut Vec<String>,
) -> (Check, usize) {
    read_back_check_with(expected, read_back, &ReadBackJudge::Strict, warnings)
}

/// One resource's verdict.
enum Verdict {
    Pass,
    Red(Finding),
    /// The finding and the `warnings` line.
    Warn(Finding, String),
}

/// The detail of a missing resource.
const MISSING_DETAIL: &str = "not in the gateway's effective view";

/// Judge one compared resource: `after` is its post read (never `System`),
/// `paths` its uncapped difference from the declared file (`None` when it is
/// missing).
fn judge_resource(
    key: &str,
    after: &Remote,
    paths: Option<&BTreeSet<String>>,
    judge: &ReadBackJudge<'_>,
) -> Verdict {
    let strict = || match paths {
        None => Verdict::Red(finding("resource_missing", key, MISSING_DETAIL)),
        Some(p) if p.is_empty() => Verdict::Pass,
        Some(p) => Verdict::Red(finding(
            "resource_differs",
            key,
            format!("differs at: {}", join_set(p)),
        )),
    };
    let warn = |why: &str| match paths {
        None => Verdict::Warn(
            finding("resource_missing", key, format!("{MISSING_DETAIL} ({why})")),
            format!("read_back: {key} is {MISSING_DETAIL} ({why})"),
        ),
        Some(p) if p.is_empty() => Verdict::Pass,
        Some(p) => {
            let at = join_set(p);
            Verdict::Warn(
                finding("resource_differs", key, format!("differs at: {at} ({why})")),
                format!("read_back: {key} differs from its file at: {at} ({why})"),
            )
        }
    };
    match judge {
        ReadBackJudge::Strict => strict(),
        ReadBackJudge::Standalone { file_status } => {
            if file_status.get(key) == Some(&PlanStatus::Same) {
                match paths {
                    None => warn("the live files equal the repo"),
                    Some(_) => {
                        warn("the live files equal the repo; the gateway's REST view differs")
                    }
                }
            } else {
                strict()
            }
        }
        ReadBackJudge::Baseline(baseline) => {
            let Some(entry) = baseline.entry(key) else {
                return strict();
            };
            let Some(p) = paths else {
                // Missing now: a warning only when it was missing before too
                // and the run did not write it.
                return match (entry.basis, &entry.before) {
                    (BaselineBasis::DeclaredFile, Remote::Missing) => {
                        warn("it was not there before this apply either")
                    }
                    _ => strict(),
                };
            };
            if p.is_empty() {
                return Verdict::Pass;
            }
            match (entry.basis, &entry.before, after) {
                (BaselineBasis::DeclaredFile, Remote::Config(r0), Remote::Config(r1)) => {
                    let moved = rest_moved(r0, r1);
                    if moved.is_empty() {
                        warn("already different before this apply")
                    } else {
                        Verdict::Red(finding(
                            "resource_differs",
                            key,
                            format!(
                                "differs at: {}; its REST view changed during this apply at: {}",
                                join_set(p),
                                join_set(&moved)
                            ),
                        ))
                    }
                }
                (BaselineBasis::DeclaredFile, _, _) => Verdict::Red(finding(
                    "resource_differs",
                    key,
                    format!(
                        "differs at: {}; its REST view changed during this apply at: {ROOT_PATH}",
                        join_set(p)
                    ),
                )),
                (BaselineBasis::LiveFile, Remote::Config(r0), Remote::Config(r1)) => {
                    let new: BTreeSet<String> = p
                        .iter()
                        .filter(|path| !path_covered(path, &entry.differs_at))
                        .cloned()
                        .collect();
                    if !new.is_empty() {
                        return Verdict::Red(finding(
                            "resource_differs",
                            key,
                            format!(
                                "differs at: {} (not different before this apply)",
                                join_set(&new)
                            ),
                        ));
                    }
                    let moved = rest_moved(r0, r1);
                    if !entry.writes_at.is_empty() && moved.is_empty() {
                        // A1: the whole view stood still.
                        return Verdict::Red(finding(
                            "resource_differs",
                            key,
                            format!(
                                "differs at: {} (its REST view did not change during this apply, so the write did not take effect)",
                                join_set(p)
                            ),
                        ));
                    }
                    // CR-01: per written path, a difference still there and
                    // no REST movement there.
                    let stuck: BTreeSet<String> = entry
                        .writes_at
                        .iter()
                        .filter(|w| p.iter().any(|q| paths_overlap(q, w)))
                        .filter(|w| !moved.iter().any(|m| paths_overlap(m, w)))
                        .cloned()
                        .collect();
                    if stuck.is_empty() {
                        warn("already different before this apply")
                    } else {
                        Verdict::Red(finding(
                            "resource_differs",
                            key,
                            format!(
                                "differs at: {} (its REST view did not change there during this apply, so that part of the write did not take effect)",
                                join_set(&stuck)
                            ),
                        ))
                    }
                }
                _ => strict(),
            }
        }
    }
}

/// The uncapped, deduplicated paths at which `rest` differs from `compare`
/// (N1 to N3, unchanged).
fn read_back_paths(compare: &Compare, rest: &Value) -> BTreeSet<String> {
    let mut paths = Vec::new();
    match compare {
        Compare::Exact(file) => {
            diff_exact(&strip_nulls(file), &strip_nulls(rest), "", &mut paths);
        }
        Compare::NameKeyedSubset(file) => {
            diff_subset(&strip_nulls(file), &strip_nulls(rest), "", &mut paths);
        }
        // the key lookup needs the RAW map (see `diff_attributes`)
        Compare::Attributes(attrs) => diff_attributes(attrs, rest, &mut paths),
    }
    paths.into_iter().collect()
}

/// The paths at which two REST views differ, nulls stripped.
fn rest_moved(before: &Value, after: &Value) -> BTreeSet<String> {
    let mut paths = Vec::new();
    diff_exact(&strip_nulls(before), &strip_nulls(after), "", &mut paths);
    paths.into_iter().collect()
}

/// The uncapped paths at which a written unit's desired comparison differs
/// from its pre-apply live one (what the write changes), nulls stripped, by
/// [`diff_exact`] on the two documents. Comparisons of different kinds differ
/// at the root.
fn write_paths(desired: &Compare, live: &Compare) -> BTreeSet<String> {
    fn parts(c: &Compare) -> (u8, Value) {
        match c {
            Compare::Exact(v) => (0, strip_nulls(v)),
            Compare::NameKeyedSubset(v) => (1, strip_nulls(v)),
            Compare::Attributes(m) => (2, strip_nulls(&Value::Object(m.clone()))),
        }
    }
    let ((kd, vd), (kl, vl)) = (parts(desired), parts(live));
    if kd != kl {
        return BTreeSet::from([ROOT_PATH.to_string()]);
    }
    let mut paths = Vec::new();
    diff_exact(&vd, &vl, "", &mut paths);
    paths.into_iter().collect()
}

fn join_set(paths: &BTreeSet<String>) -> String {
    join_paths(paths.iter().cloned().collect())
}

/// `value` with every JSON `null` object member removed, at every depth (N1).
/// Applied to the file value and to the REST value alike, so a null member
/// equals an absent one on either side.
fn strip_nulls(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), strip_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_nulls).collect()),
        other => other.clone(),
    }
}

fn child_path(path: &str, key: &str) -> String {
    // the key may be gateway-supplied (REST body): no control characters and
    // a bounded length, like every other gateway string in a finding
    let key = printable(key);
    if path.is_empty() {
        key
    } else {
        format!("{path}.{key}")
    }
}

fn root_or(path: &str) -> String {
    if path.is_empty() {
        ROOT_PATH.to_string()
    } else {
        path.to_string()
    }
}

/// JSON number equality that does not tell `1` from `1.0`.
fn same_scalar(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            // `1` equals `1.0`, but two integers are compared exactly: above
            // 2^53 distinct i64/u64 values collapse to one f64 (WR-06).
            if x.is_f64() || y.is_f64() {
                matches!((x.as_f64(), y.as_f64()), (Some(x), Some(y)) if x == y)
            } else {
                x == y
            }
        }
        _ => a == b,
    }
}

/// The JSON paths at which `file` and `rest` differ (key order ignored).
fn diff_exact(file: &Value, rest: &Value, path: &str, out: &mut Vec<String>) {
    match (file, rest) {
        (Value::Object(f), Value::Object(r)) => {
            for (key, fv) in f {
                match r.get(key) {
                    Some(rv) => diff_exact(fv, rv, &child_path(path, key), out),
                    None => out.push(child_path(path, key)),
                }
            }
            for key in r.keys().filter(|k| !f.contains_key(*k)) {
                out.push(child_path(path, key));
            }
        }
        (Value::Array(f), Value::Array(r)) if f.len() == r.len() => {
            for (i, (fv, rv)) in f.iter().zip(r).enumerate() {
                diff_exact(fv, rv, &child_path(path, &i.to_string()), out);
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {
            out.push(root_or(path));
        }
        (f, r) => {
            if !same_scalar(f, r) {
                out.push(root_or(path));
            }
        }
    }
}

/// N2: every part of `file` must appear in `rest`; arrays of named objects
/// are matched by `name`, other array elements must appear somewhere.
fn diff_subset(file: &Value, rest: &Value, path: &str, out: &mut Vec<String>) {
    match (file, rest) {
        (Value::Object(f), Value::Object(r)) => {
            for (key, fv) in f {
                match r.get(key) {
                    Some(rv) => diff_subset(fv, rv, &child_path(path, key), out),
                    None => out.push(child_path(path, key)),
                }
            }
        }
        (Value::Array(f), Value::Array(r)) => {
            for (i, fv) in f.iter().enumerate() {
                let name = fv.get("name").and_then(Value::as_str);
                if let (Some(name), true) = (name, fv.is_object()) {
                    let at = format!("{path}[{}]", printable(name));
                    match r
                        .iter()
                        .find(|rv| rv.get("name").and_then(Value::as_str) == Some(name))
                    {
                        Some(rv) => diff_subset(fv, rv, &at, out),
                        None => out.push(at),
                    }
                } else {
                    let found = r.iter().any(|rv| {
                        let mut inner = Vec::new();
                        diff_exact(fv, rv, "", &mut inner);
                        inner.is_empty()
                    });
                    if !found {
                        out.push(child_path(path, &i.to_string()));
                    }
                }
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {
            out.push(root_or(path));
        }
        (f, r) => {
            if !same_scalar(f, r) {
                out.push(root_or(path));
            }
        }
    }
}

/// N3: the attribute keys the REST config also carries must be equal.
///
/// The key lookup is on the RAW REST map: a REST `null` for a declared key is
/// a present key, so it is compared (and differs from a declared value), not
/// skipped as absent. Only the values are null-stripped (N1).
fn diff_attributes(attrs: &Map<String, Value>, rest: &Value, out: &mut Vec<String>) {
    let Some(rest) = rest.as_object() else {
        out.push("(root)".to_string());
        return;
    };
    for (key, expected) in attrs {
        if let Some(seen) = rest.get(key) {
            let mut inner = Vec::new();
            diff_exact(&strip_nulls(expected), &strip_nulls(seen), "", &mut inner);
            if !inner.is_empty() {
                out.push(key.clone());
            }
        }
    }
}

fn join_paths(mut paths: Vec<String>) -> String {
    paths.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    paths.dedup();
    let more = paths.len().saturating_sub(MAX_PATHS);
    paths.truncate(MAX_PATHS);
    let mut text = paths.join(", ");
    if more > 0 {
        text.push_str(&format!(" (+{more} more)"));
    }
    text
}

// ---- 4. files -----------------------------------------------------------------

/// The `unreadable` finding's detail.
const UNREADABLE_DETAIL: &str =
    "the unit has a live path the host's sftp server cannot read; its live copy is unknown";

fn check_files(expected: &Expected, observed: &ObservedState, warnings: &mut Vec<String>) -> Check {
    let mut findings = Vec::new();
    for unit in &expected.tag_units {
        // Never absent: an unreadable unit is not "not found".
        if observed.unreadable.iter().any(|(key, _)| key == unit) {
            findings.push(finding("unreadable", unit.clone(), UNREADABLE_DETAIL));
            continue;
        }
        let status = observed
            .files
            .iter()
            .find(|(key, _)| key == unit)
            .map(|(_, status)| *status);
        match status {
            Some(PlanStatus::Same) => {}
            Some(other) => findings.push(finding(
                "files_differ",
                unit.clone(),
                format!(
                    "the effective live copy is `{}` against the render",
                    other.word()
                ),
            )),
            None => findings.push(finding(
                "files_differ",
                unit.clone(),
                "the unit was not found among the live files",
            )),
        }
    }
    for (key, declared) in &observed.unreadable {
        if expected.tag_units.contains(key) {
            continue;
        }
        if *declared {
            findings.push(finding("unreadable", key.clone(), UNREADABLE_DETAIL));
        } else {
            warnings.push(format!(
                "files: {key} is unreadable on the host and is not declared; it was not compared"
            ));
        }
    }
    sort_findings(&mut findings);
    finished(
        CheckId::Files,
        format!(
            "{} declared tag unit(s) compared byte for byte with the live files",
            expected.tag_units.len()
        ),
        findings,
    )
}

// ---- 5. loaded ----------------------------------------------------------------

fn check_loaded(expected: &Expected, observed: &ObservedState) -> Check {
    if observed.scan.scan_active {
        return Check {
            id: CheckId::Loaded,
            status: CheckStatus::Warn,
            detail: "not observed: a scan is running, so its log lines are not readable yet"
                .to_string(),
            findings: Vec::new(),
        };
    }
    let lines = match &observed.logs {
        Ok(lines) => lines,
        Err(slug) => {
            return finished(
                CheckId::Loaded,
                "the gateway log could not be read".to_string(),
                vec![finding("logs_unavailable", "logs", printable(slug))],
            );
        }
    };

    // The longest declared id that prefixes the resource text wins the line,
    // so `Prod` does not claim a line about `Prod Backup`.
    let mut hits: BTreeMap<&str, (i64, &str)> = BTreeMap::new();
    for line in lines {
        if !line.message.contains(WILL_NOT_BE_LOADED) {
            continue;
        }
        let owner = expected
            .declared_ids
            .iter()
            .filter(|id| line.message.contains(&format!("Resource {id} ")))
            .max_by_key(|id| id.len());
        if let Some(id) = owner {
            let entry = hits
                .entry(id.as_str())
                .or_insert((line.timestamp, line.logger.as_str()));
            if line.timestamp < entry.0 {
                *entry = (line.timestamp, line.logger.as_str());
            }
        }
    }
    let mut findings: Vec<Finding> = hits
        .into_iter()
        .map(|(id, (timestamp, logger))| {
            finding(
                "unloaded_resource",
                id,
                format!("logged at {timestamp} by `{}`", printable(logger)),
            )
        })
        .collect();
    sort_findings(&mut findings);
    let detail = if findings.is_empty() {
        format!(
            "no `{WILL_NOT_BE_LOADED}` line for a declared resource since the last scan (the gateway logs it once per scan that finds the resource, and not for every type, so this covers the last scan only)"
        )
    } else {
        format!("{} declared resource(s) were not loaded", findings.len())
    };
    finished(CheckId::Loaded, detail, findings)
}

// ---- 6. probes ----------------------------------------------------------------

fn check_probes(expected: &Expected, observed: &ObservedState) -> Check {
    if expected.probes.is_empty() {
        return Check {
            id: CheckId::Probes,
            status: CheckStatus::Warn,
            detail: "the manifest declares no `verify.tag_probes`; nothing was probed".to_string(),
            findings: vec![finding(
                "tag_probes_absent",
                "verify.tag_probes",
                "add tag paths to the manifest to probe them after a deploy",
            )],
        };
    }
    let readings = match &observed.probes {
        Some(Ok(readings)) => readings,
        Some(Err(slug)) => {
            return finished(
                CheckId::Probes,
                "the probes could not be read".to_string(),
                vec![finding("tag_probes_unavailable", "tags", printable(slug))],
            );
        }
        None => {
            return finished(
                CheckId::Probes,
                "the probes could not be read".to_string(),
                vec![finding("tag_probes_unavailable", "tags", "not read")],
            );
        }
    };
    let mut findings = Vec::new();
    for path in &expected.probes {
        match readings.iter().find(|r| &r.path == path) {
            Some(r) if r.quality.starts_with("Good") => {}
            Some(r) => findings.push(finding(
                "tag_probe_not_good",
                path.clone(),
                format!("quality {}", quality_code(&r.quality)),
            )),
            None => findings.push(finding("tag_probe_not_good", path.clone(), "no reading")),
        }
    }
    // Manifest order, not sorted.
    finished(
        CheckId::Probes,
        format!("{} tag probe(s) read", expected.probes.len()),
        findings,
    )
}

/// A quality string reduced to a code that cannot carry free text: the class
/// (`Good`, `Bad`, `Uncertain`, `Error`) and, when the rest is plain letters,
/// the Ignition suffix (`Bad_NotFound`).
fn quality_code(quality: &str) -> String {
    if quality.is_empty() {
        return "empty".to_string();
    }
    let (class, rest) = match quality.split_once('_') {
        Some((class, rest)) => (class, Some(rest)),
        None => (quality, None),
    };
    if !matches!(class, "Good" | "Bad" | "Uncertain" | "Error") {
        return "unrecognised".to_string();
    }
    match rest {
        Some(rest)
            if !rest.is_empty()
                && rest.len() <= 48
                && rest.chars().all(|c| c.is_ascii_alphabetic() || c == '_') =>
        {
            format!("{class}_{rest}")
        }
        _ => class.to_string(),
    }
}

/// `text` limited to printable characters and [`MAX_CODE_CHARS`], for the few
/// gateway-supplied names that reach a finding (logger names, error slugs,
/// the active mode).
fn printable(text: &str) -> String {
    let mut out: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CODE_CHARS)
        .collect();
    if text.chars().count() > MAX_CODE_CHARS {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The renderer words are the serialized words, so the human and JSON
    /// forms never drift.
    #[test]
    fn words_equal_the_serialized_forms() {
        for id in [
            CheckId::Scan,
            CheckId::Mode,
            CheckId::ReadBack,
            CheckId::Files,
            CheckId::Loaded,
            CheckId::Probes,
        ] {
            assert_eq!(json!(id), json!(id.word()));
        }
        for status in [CheckStatus::Pass, CheckStatus::Fail, CheckStatus::Warn] {
            assert_eq!(json!(status), json!(status.word()));
        }
    }

    #[test]
    fn quality_codes_carry_no_free_text() {
        assert_eq!(quality_code("Good"), "Good");
        assert_eq!(quality_code("Good_Provisional"), "Good_Provisional");
        assert_eq!(quality_code("Bad_NotFound"), "Bad_NotFound");
        assert_eq!(quality_code("Bad_secret-123"), "Bad");
        assert_eq!(quality_code("Weird"), "unrecognised");
        assert_eq!(quality_code(""), "empty");
    }

    #[test]
    fn printable_drops_controls_and_caps_length() {
        assert_eq!(printable("a\nb\u{7}c"), "abc");
        let long = "x".repeat(MAX_CODE_CHARS + 10);
        assert_eq!(printable(&long).len(), MAX_CODE_CHARS + 3);
    }

    #[test]
    fn gateway_supplied_keys_are_sanitised_in_diff_paths() {
        // WR-05: a REST key with an escape sequence or a huge length must not
        // reach a finding raw.
        let hostile = format!("evil\u{1b}[31m{}", "k".repeat(MAX_CODE_CHARS * 2));
        let mut rest = serde_json::Map::new();
        rest.insert(hostile, json!(1));
        let mut out = Vec::new();
        diff_exact(&json!({}), &Value::Object(rest), "", &mut out);
        assert_eq!(out.len(), 1);
        assert!(!out[0].contains('\u{1b}'), "{:?}", out[0]);
        assert!(
            out[0].chars().count() <= MAX_CODE_CHARS + 3,
            "{}",
            out[0].len()
        );
    }

    #[test]
    fn large_integers_compare_exactly() {
        // WR-06: 2^53 + 1 and 2^53 are the same f64 but different integers.
        let mut out = Vec::new();
        diff_exact(
            &json!({"id": 9_007_199_254_740_993_i64}),
            &json!({"id": 9_007_199_254_740_992_i64}),
            "",
            &mut out,
        );
        assert_eq!(out, vec!["id".to_string()]);
        let mut out = Vec::new();
        diff_exact(
            &json!({"id": u64::MAX}),
            &json!({"id": u64::MAX - 1}),
            "",
            &mut out,
        );
        assert_eq!(out, vec!["id".to_string()]);
        let mut out = Vec::new();
        diff_exact(
            &json!({"id": 9_007_199_254_740_993_i64}),
            &json!({"id": 9_007_199_254_740_993_i64}),
            "",
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn numbers_compare_by_value() {
        let mut out = Vec::new();
        diff_exact(
            &json!({"a": 1, "b": 2.5}),
            &json!({"a": 1.0, "b": 2.5}),
            "",
            &mut out,
        );
        assert!(out.is_empty(), "{out:?}");
        diff_exact(&json!({"a": 1}), &json!({"a": 2}), "", &mut out);
        assert_eq!(out, vec!["a".to_string()]);
    }

    #[test]
    fn path_list_is_sorted_deduplicated_and_capped() {
        let paths: Vec<String> = (0..MAX_PATHS + 3)
            .rev()
            .map(|i| format!("p{i:02}"))
            .collect();
        let text = join_paths(paths);
        assert!(text.starts_with("p00, p01"), "{text}");
        assert!(text.ends_with("(+3 more)"), "{text}");
    }
}
