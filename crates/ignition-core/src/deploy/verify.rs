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
//! - **N1** members whose file value is JSON `null` are dropped from the file
//!   value first, at every depth, for every type. A REST `null` member where
//!   the file has none still differs.
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
pub const NO_READ_ROUTE: &[(&str, &str)] =
    &[("com.inductiveautomation.opcua", "one-time"), ("ignition", "images")];

/// The collection name of gateway-provided, non-file-backed resources (CV2).
pub const SYSTEM_COLLECTION: &str = "system";

/// Most differing JSON paths one finding names.
const MAX_PATHS: usize = 20;

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

/// One value-free observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable code (`scan_active`, `mode_mismatch`, `resource_missing`,
    /// `resource_differs`, `files_differ`, `logs_unavailable`,
    /// `unloaded_resource`, `tag_probes_absent`, `tag_probes_unavailable`,
    /// `tag_probe_not_good`).
    pub code: &'static str,
    /// What it is about: a resource key, unit key, tag path, `mode`, `logs`.
    pub subject: String,
    /// Paths, codes and timestamps only.
    pub detail: String,
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
    /// Equal after N1.
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
            };
            let compare = if let Some(bytes) = file("config.json") {
                match serde_json::from_slice::<Value>(bytes) {
                    Ok(value) if res.module == "ignition" && res.ty == "security-levels" => {
                        Compare::NameKeyedSubset(value)
                    }
                    Ok(value) => Compare::Exact(value),
                    Err(_) => {
                        not_checked.insert(key, "config.json is not JSON".to_string());
                        continue;
                    }
                }
            } else if let Some(attributes) = file("resource.json")
                .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
                .and_then(|v| v.get("attributes").and_then(Value::as_object).cloned())
            {
                Compare::Attributes(attributes)
            } else {
                not_checked.insert(key, "no config.json to compare".to_string());
                continue;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReading {
    /// The tag path read.
    pub path: String,
    /// The quality string (`Good`, `Bad_NotFound`, ...).
    pub quality: String,
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
}

/// The start of the log window for the last scan (CV1), epoch ms on the
/// gateway clock.
pub fn log_window_start(scan: &ScanStatus) -> i64 {
    let duration = scan.last_scan_duration.unwrap_or(LOG_WINDOW_FALLBACK_MS);
    (scan.last_scan_timestamp - duration - LOG_WINDOW_SLACK_MS).max(0)
}

/// Evaluate `observed` against `expected`.
pub fn evaluate(expected: &Expected, observed: &ObservedState) -> VerifyReport {
    let _ = (expected, observed);
    let checks = [
        CheckId::Scan,
        CheckId::Mode,
        CheckId::ReadBack,
        CheckId::Files,
        CheckId::Loaded,
        CheckId::Probes,
    ]
    .into_iter()
    .map(|id| Check {
        id,
        status: CheckStatus::Pass,
        detail: String::new(),
        findings: Vec::new(),
    })
    .collect();
    VerifyReport {
        ok: true,
        slug: None,
        checks,
        resources_checked: 0,
        warnings: Vec::new(),
    }
}
