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
}

/// The start of the log window for the last scan (CV1), epoch ms on the
/// gateway clock.
pub fn log_window_start(scan: &ScanStatus) -> i64 {
    let duration = scan.last_scan_duration.unwrap_or(LOG_WINDOW_FALLBACK_MS);
    (scan.last_scan_timestamp - duration - LOG_WINDOW_SLACK_MS).max(0)
}

/// Evaluate `observed` against `expected`.
pub fn evaluate(expected: &Expected, observed: &ObservedState) -> VerifyReport {
    let mut warnings = Vec::new();
    let (read_back, resources_checked) = check_read_back(expected, observed, &mut warnings);
    let checks = vec![
        check_scan(observed),
        check_mode(expected, observed),
        read_back,
        check_files(expected, observed),
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
    if observed.mode.as_deref() == Some(expected.mode.as_str()) {
        return finished(
            CheckId::Mode,
            format!("the gateway runs mode `{}`", expected.mode),
            Vec::new(),
        );
    }
    let seen = match &observed.mode {
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

fn check_read_back(
    expected: &Expected,
    observed: &ObservedState,
    warnings: &mut Vec<String>,
) -> (Check, usize) {
    let mut findings = Vec::new();
    let mut checked = 0usize;
    for res in &expected.resources {
        match observed.read_back.get(&res.key).unwrap_or(&Remote::Missing) {
            Remote::Missing => {
                checked += 1;
                findings.push(finding(
                    "resource_missing",
                    res.key.clone(),
                    "not in the gateway's effective view",
                ));
            }
            Remote::System => warnings.push(format!(
                "read_back: {} not compared: the effective copy is in the `{SYSTEM_COLLECTION}` collection (gateway-provided, not file-backed)",
                res.key
            )),
            Remote::Config(rest) => {
                checked += 1;
                let mut paths = Vec::new();
                match &res.compare {
                    Compare::Exact(file) => {
                        let rest = strip_nulls(rest);
                        diff_exact(&strip_nulls(file), &rest, "", &mut paths);
                    }
                    Compare::NameKeyedSubset(file) => {
                        let rest = strip_nulls(rest);
                        diff_subset(&strip_nulls(file), &rest, "", &mut paths);
                    }
                    // the key lookup needs the RAW map (see `diff_attributes`)
                    Compare::Attributes(attrs) => diff_attributes(attrs, rest, &mut paths),
                }
                if !paths.is_empty() {
                    findings.push(finding(
                        "resource_differs",
                        res.key.clone(),
                        format!("differs at: {}", join_paths(paths)),
                    ));
                }
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
    let detail = format!("{checked} declared resource(s) compared with the REST effective view");
    (finished(CheckId::ReadBack, detail, findings), checked)
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
        "(root)".to_string()
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

fn check_files(expected: &Expected, observed: &ObservedState) -> Check {
    let mut findings = Vec::new();
    for unit in &expected.tag_units {
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
