//! JVM profiling actions — `ign jvm sample` / `ign jvm jfr
//! start|status|stop`. **This is a SPECIAL MODE, not a diagnostic:**
//! every verb here ACTIVELY instruments the gateway JVM (stack
//! sampling burns CPU proportional to thread count × frequency; JFR
//! buffers in JVM memory and dumps to gateway disk). The family is
//! therefore gated HARDER than the destructive verbs: the global
//! `--yes`/`IGNITION_YES` does NOT satisfy it — only the explicit
//! `--i-accept-the-risk` flag (CLI) or the typed `ACKNOWLEDGE`
//! interstitial (TUI) does, and the refusal is the additive
//! `profiling_risk_not_accepted` (exit 2 — the usage class: it names
//! a flag the caller must add, the ConfirmationRequired precedent).
//!
//! Two mechanisms, both riding the ALREADY-deployed, secret-gated
//! scriptExec WebDev route (05-01's contract — the structural opt-in
//! from `ign webdev deploy --with-script-exec`; no new route, no new
//! server-side surface):
//!
//! 1. **Sampling thread profiler** (`sample`) — a Jython loop inside
//!    the gateway JVM calls `ThreadMXBean.dumpAllThreads(false,
//!    false)` every `interval_ms` for `duration_secs`, aggregating
//!    self-counts per top frame and occurrence counts per full stack
//!    (folded format — flamegraph-ready). Pure JVM READS: no JVM
//!    flags, no agents, no disk.
//! 2. **Java Flight Recorder** (`jfr`) — the
//!    `com.sun.management:type=DiagnosticCommand` MBean's
//!    `jfrStart`/`jfrCheck`/`jfrDump`/`jfrStop` (the jcmd-over-MBean
//!    trick). `start` is duration-capped and memory-buffered (no
//!    `filename=` at start); `stop` dumps to the gateway's temp dir,
//!    streams the bytes back base64 (the 11-02/11-04 carrier — the
//!    ONLY transfer transformation), and deletes the remote file.
//!
//! HARD CAPS (the special-mode contract — REFUSED, never clamped, so
//! the caller sees the boundary): sample duration ≤ 120 s, sample
//! interval ≥ 5 ms, JFR duration ≤ 300 s, JFR dump ≤ 32 MiB (a bigger
//! recording stays on the gateway with its path reported — fetched
//! false, remote_deleted false, never silently truncated).
//!
//! The long exec calls ride `webdev_route_call_with_timeout` (the
//! bundle-download precedent): a 120 s sampling run would die to the
//! 30 s client default otherwise. The timeout is duration + a fixed
//! margin, never unlimited.
//!
//! Every result model leads with `warning` = [`PRODUCTION_WARNING`]
//! (ALL keys always — the family convention): the production caution
//! rides every JSON envelope and every human render, so a piped/agent
//! consumer can never miss it.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;

use crate::actions::webdev::{SCRIPT_EXEC_ROUTE, SECRET_HEADER};
use crate::client::GatewayApi;
use crate::config::Config;
use crate::error::CoreError;

/// THE production caution — the single source of truth rendered by
/// the CLI (human banner + every envelope's `warning` field) and the
/// TUI (the gate interstitial + the screen's persistent banner).
pub const PRODUCTION_WARNING: &str = "JVM PROFILING ACTIVELY INSTRUMENTS THE GATEWAY: sampling adds CPU overhead \
     proportional to thread count x frequency; flight recordings buffer in JVM \
     memory and dump to gateway disk. NEVER use this on a production server \
     outside an approved change window with a rollback plan — and always use \
     the shortest duration that answers the question.";

/// The CLI flag name the refusal prose names (one spelling, pinned by
/// the contract goldens).
pub const RISK_FLAG: &str = "--i-accept-the-risk";

/// The one flight-recording name this CLI manages (stop/status key on
/// it; a collision with a foreign recording is refused route-side).
pub const JFR_RECORDING_NAME: &str = "ign-cli";

// ── HARD CAPS (refused, never clamped) ─────────────────────────────

/// Sampling duration: default 30 s, floor 5 s (fewer samples is
/// noise), HARD CAP 120 s (the special-mode contract).
pub const SAMPLE_DURATION_DEFAULT_SECS: u64 = 30;
/// Sampling duration floor.
pub const SAMPLE_DURATION_MIN_SECS: u64 = 5;
/// Sampling duration HARD CAP.
pub const SAMPLE_DURATION_MAX_SECS: u64 = 120;
/// Sampling interval: default 20 ms, floor 5 ms (below that the
/// sampler thread itself skews the JVM), cap 1000 ms.
pub const SAMPLE_INTERVAL_DEFAULT_MS: u64 = 20;
/// Sampling interval floor.
pub const SAMPLE_INTERVAL_MIN_MS: u64 = 5;
/// Sampling interval cap.
pub const SAMPLE_INTERVAL_MAX_MS: u64 = 1000;
/// Default `--top` hot-method rows.
pub const HOT_METHODS_DEFAULT: u64 = 25;
/// `--top` cap (payload bound).
pub const HOT_METHODS_MAX: u64 = 100;
/// Distinct stacks kept server-side (folded payload bound).
pub const TOP_STACKS_KEPT: u64 = 200;
/// Frames kept per stack (depth bound).
pub const MAX_STACK_FRAMES: u64 = 64;

/// JFR duration: default 60 s, floor 10 s, HARD CAP 300 s.
pub const JFR_DURATION_DEFAULT_SECS: u64 = 60;
/// JFR duration floor.
pub const JFR_DURATION_MIN_SECS: u64 = 10;
/// JFR duration HARD CAP.
pub const JFR_DURATION_MAX_SECS: u64 = 300;
/// JFR dump size cap — a bigger file is NOT streamed (reported in
/// place on the gateway instead).
pub const JFR_DUMP_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Margin added to a sampling duration for the exec request's
/// per-request timeout (route overhead + envelope transfer).
const EXEC_TIMEOUT_MARGIN_SECS: u64 = 30;

// ── Options & result models (ALL keys always) ──────────────────────

/// `ign jvm sample` options (validated by [`validate_sample_options`]
/// BEFORE the risk gate or any I/O — usage errors lead).
#[derive(Debug, Clone, Copy)]
pub struct JvmSampleOptions {
    /// Seconds to sample (5..=120).
    pub duration_secs: u64,
    /// Milliseconds between thread dumps (5..=1000).
    pub interval_ms: u64,
    /// Hot-method rows to return (1..=100).
    pub top: u64,
}

impl Default for JvmSampleOptions {
    fn default() -> Self {
        Self {
            duration_secs: SAMPLE_DURATION_DEFAULT_SECS,
            interval_ms: SAMPLE_INTERVAL_DEFAULT_MS,
            top: HOT_METHODS_DEFAULT,
        }
    }
}

/// `ign jvm jfr start` options.
#[derive(Debug, Clone, Copy)]
pub struct JvmJfrStartOptions {
    /// Recording duration in seconds (10..=300) — the recording
    /// auto-stops at the deadline even if `stop` never runs (the
    /// forgetfulness guard).
    pub duration_secs: u64,
    /// The JDK's bundled settings name — `default` (lower overhead)
    /// or `profile` (deeper, heavier). Whitelisted at the CLI; the
    /// builder accepts only these two so no free text ever reaches
    /// the MBean invocation.
    pub settings: JfrSettings,
}

impl Default for JvmJfrStartOptions {
    fn default() -> Self {
        Self {
            duration_secs: JFR_DURATION_DEFAULT_SECS,
            settings: JfrSettings::Default,
        }
    }
}

/// The two bundled JFR settings files (the whitelist — a free-text
/// `settings=` would be an injection vector into the MBean call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JfrSettings {
    /// `settings=default` — the lower-overhead bundled config.
    Default,
    /// `settings=profile` — the deeper bundled config.
    Profile,
}

impl JfrSettings {
    /// The MBean argument token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Profile => "profile",
        }
    }
}

/// One hot-method row (self-time proxy: thread-stack observations
/// whose TOP frame is this method).
#[derive(Debug, Serialize)]
pub struct HotMethod {
    /// `com.example.Foo.bar` (class.method — line numbers ride the
    /// stacks, not this rollup).
    pub frame: String,
    /// Observed thread stacks with this frame on top.
    pub count: u64,
    /// `count / observations * 100`, one decimal.
    pub pct: f64,
}

/// One aggregated stack (folded format: root;…;leaf, space, count).
#[derive(Debug, Serialize)]
pub struct HotStack {
    /// Semicolon-joined frames, root first.
    pub stack: String,
    /// Observed occurrences of exactly this stack.
    pub count: u64,
    /// `count / observations * 100`, one decimal.
    pub pct: f64,
}

/// `ign jvm sample` result — `warning` FIRST (declaration order =
/// golden field order).
#[derive(Debug, Serialize)]
pub struct JvmSampleResult {
    /// [`PRODUCTION_WARNING`] — rides every render path.
    pub warning: &'static str,
    /// The requested duration (validated).
    pub duration_secs: u64,
    /// The requested interval (validated — the EFFECTIVE cadence is
    /// longer: each dump costs wall time the sleep adds to).
    pub interval_ms: u64,
    /// Thread dumps taken.
    pub samples: u64,
    /// Peak live threads seen in one dump.
    pub thread_peak: u64,
    /// Total thread stacks observed across all dumps — the
    /// denominator every `pct` rides (live-proven: per-SAMPLE
    /// normalization is nonsense for per-THREAD counts).
    pub observations: u64,
    /// Measured wall time of the sampling loop in milliseconds
    /// (gateway-side clock — the honest "what the window actually
    /// cost", dump overhead included).
    pub wall_ms: u64,
    /// Top-N self-time rollup.
    pub hot_methods: Vec<HotMethod>,
    /// The heaviest distinct stacks (≤ [`TOP_STACKS_KEPT`]).
    pub top_stacks: Vec<HotStack>,
    /// Folded-stack lines (`root;…;leaf <count>` — flamegraph.pl /
    /// speedscope input), the same rows as `top_stacks`.
    pub folded: String,
    /// The route-measured wall time (the scriptExec convention).
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
}

/// `ign jvm jfr start` result.
#[derive(Debug, Serialize)]
pub struct JvmJfrStartResult {
    /// [`PRODUCTION_WARNING`].
    pub warning: &'static str,
    /// The recording name ([`JFR_RECORDING_NAME`]).
    pub name: &'static str,
    /// Auto-stop deadline (validated).
    pub duration_secs: u64,
    /// The bundled settings name used.
    pub settings: &'static str,
    /// The MBean's jfrStart answer (verbatim — the JDK's own prose).
    pub detail: String,
    /// The route-measured wall time.
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
}

/// `ign jvm jfr status` result (READ-only — carries no risk gate).
#[derive(Debug, Serialize)]
pub struct JvmJfrStatusResult {
    /// [`PRODUCTION_WARNING`] — even the read names the mode.
    pub warning: &'static str,
    /// Whether a recording named [`JFR_RECORDING_NAME`] exists.
    pub active: bool,
    /// The MBean's jfrCheck answer (verbatim — every recording the
    /// JVM knows, not just ours).
    pub recordings: String,
    /// The route-measured wall time.
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
}

/// `ign jvm jfr stop` result — stop + dump + fetch + remote cleanup.
#[derive(Debug, Serialize)]
pub struct JvmJfrStopResult {
    /// [`PRODUCTION_WARNING`].
    pub warning: &'static str,
    /// The recording name stopped.
    pub name: &'static str,
    /// The LOCAL file written — `None` when the dump exceeded
    /// [`JFR_DUMP_MAX_BYTES`] (`fetched` false; the file stays on the
    /// gateway and `remote_file` names it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Whether the bytes were streamed back (false = too large, left
    /// in place — NEVER silently truncated).
    pub fetched: bool,
    /// The dump's size on the gateway (always known, fetched or not).
    pub bytes: u64,
    /// The gateway-side path the dump was written to.
    pub remote_file: String,
    /// Whether the gateway-side file was deleted after the fetch
    /// (always false when `fetched` is false).
    pub remote_deleted: bool,
    /// The route-measured wall time.
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
}

// ── The risk gate ──────────────────────────────────────────────────

/// The special-mode gate — STRONGER than `require_confirmation`: only
/// the explicit per-invocation opt-in satisfies it (the CLI's
/// `--i-accept-the-risk`; the TUI's typed `ACKNOWLEDGE`, passed down
/// as `true`). `--yes`/`IGNITION_YES` never reaches this function —
/// that is the entire point of the separate error class.
pub fn require_risk_acceptance(accepted: bool, operation: &str) -> Result<(), CoreError> {
    if accepted {
        Ok(())
    } else {
        Err(CoreError::ProfilingRiskNotAccepted {
            operation: operation.to_string(),
        })
    }
}

// ── Validation (refuse above the caps — usage errors lead) ─────────

/// Validate the sampling options against the HARD CAPS. Above-cap
/// values are REFUSED (`invalid_input`, exit 2) with the cap named —
/// never silently clamped (the caller must SEE the boundary; a
/// clamped 10-minute run would look like consent).
pub fn validate_sample_options(opts: &JvmSampleOptions) -> Result<(), CoreError> {
    if opts.duration_secs > SAMPLE_DURATION_MAX_SECS {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "sample duration {}s exceeds the hard cap of {}s — the cap is not \
                 configurable; profile in short windows and repeat if needed",
                opts.duration_secs, SAMPLE_DURATION_MAX_SECS
            ),
        });
    }
    if opts.duration_secs < SAMPLE_DURATION_MIN_SECS {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "sample duration {}s is below the {}s floor — fewer samples is \
                 noise, not a profile",
                opts.duration_secs, SAMPLE_DURATION_MIN_SECS
            ),
        });
    }
    if opts.interval_ms > SAMPLE_INTERVAL_MAX_MS || opts.interval_ms < SAMPLE_INTERVAL_MIN_MS {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "sample interval {}ms is outside {}–{}ms (below {}ms the sampler \
                 thread itself skews the JVM)",
                opts.interval_ms,
                SAMPLE_INTERVAL_MIN_MS,
                SAMPLE_INTERVAL_MAX_MS,
                SAMPLE_INTERVAL_MIN_MS
            ),
        });
    }
    if opts.top == 0 || opts.top > HOT_METHODS_MAX {
        return Err(CoreError::InvalidInput {
            reason: format!("--top must be within 1–{HOT_METHODS_MAX}"),
        });
    }
    Ok(())
}

/// Validate the JFR start options against the HARD CAPS (same
/// refuse-don't-clamp contract as sampling).
pub fn validate_jfr_start_options(opts: &JvmJfrStartOptions) -> Result<(), CoreError> {
    if opts.duration_secs > JFR_DURATION_MAX_SECS {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "JFR duration {}s exceeds the hard cap of {}s — the cap is not \
                 configurable; record short windows and repeat if needed",
                opts.duration_secs, JFR_DURATION_MAX_SECS
            ),
        });
    }
    if opts.duration_secs < JFR_DURATION_MIN_SECS {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "JFR duration {}s is below the {}s floor",
                opts.duration_secs, JFR_DURATION_MIN_SECS
            ),
        });
    }
    Ok(())
}

// ── The Jython payloads (token-substituted templates) ──────────────
//
// Substitution safety: the ONLY substituted values are validated
// integers and the whitelisted settings token — no free text ever
// reaches the gateway-side code. The templates use __TOKEN__ markers
// (never str.format — the Jython braces would collide).

/// The sampling-profiler payload. Aggregates SERVER-SIDE (the wire
/// carries rollups, never raw dumps): `hot` self-counts per top
/// frame, `stacks` occurrence counts per full stack, `folded` the
/// flamegraph-ready rendering of `stacks`. The sampler's own request
/// thread is excluded (it would top every chart).
pub fn build_sample_code(opts: &JvmSampleOptions) -> String {
    SAMPLE_TEMPLATE
        .replace("__DURATION_MS__", &(opts.duration_secs * 1000).to_string())
        .replace("__INTERVAL_MS__", &opts.interval_ms.to_string())
        .replace("__TOP__", &opts.top.to_string())
        .replace("__MAX_STACKS__", &TOP_STACKS_KEPT.to_string())
        .replace("__MAX_FRAMES__", &MAX_STACK_FRAMES.to_string())
}

const SAMPLE_TEMPLATE: &str = r#"
import system

def _ign_jvm_sample():
    # The route surfaces the _result GLOBAL (the 05-01 exec contract)
    # — an in-function assignment needs the declaration, live-proven:
    # without it the answer's `result` comes back empty.
    global _result
    from java.lang.management import ManagementFactory
    from java.lang import Thread
    import time
    bean = ManagementFactory.getThreadMXBean()
    me = Thread.currentThread().getName()
    start = time.time()
    deadline = start + (__DURATION_MS__ / 1000.0)
    interval = __INTERVAL_MS__ / 1000.0
    self_counts = {}
    stack_counts = {}
    samples = 0
    thread_peak = 0
    observations = 0
    while True:
        infos = bean.dumpAllThreads(False, False)
        live = 0
        for info in infos:
            if info is None:
                continue
            if info.getThreadName() == me:
                continue
            trace = info.getStackTrace()
            if trace is None or len(trace) == 0:
                continue
            live += 1
            frames = []
            limit = min(len(trace), __MAX_FRAMES__)
            for i in range(limit):
                el = trace[i]
                frames.append(str(el.getClassName()) + '.' + str(el.getMethodName()))
            top = frames[0]
            self_counts[top] = self_counts.get(top, 0) + 1
            frames.reverse()
            key = ';'.join(frames)
            stack_counts[key] = stack_counts.get(key, 0) + 1
        samples += 1
        observations += live
        if live > thread_peak:
            thread_peak = live
        if time.time() >= deadline:
            break
        time.sleep(interval)
    wall_ms = int((time.time() - start) * 1000)
    hot = sorted(self_counts.items(), key=lambda kv: kv[1], reverse=True)[:__TOP__]
    stacks = sorted(stack_counts.items(), key=lambda kv: kv[1], reverse=True)[:__MAX_STACKS__]
    folded = '\n'.join([k + ' ' + str(v) for k, v in stacks])
    _result = system.util.jsonEncode({
        'samples': samples,
        'threadPeak': thread_peak,
        'observations': observations,
        'wallMs': wall_ms,
        'hot': [{'frame': k, 'count': v} for k, v in hot],
        'stacks': [{'stack': k, 'count': v} for k, v in stacks],
        'folded': folded,
    })

_ign_jvm_sample()
"#;

/// The shared DiagnosticCommand-MBean prelude for the JFR family
/// (jcmd-over-MBean: every operation is `(String[]) -> String` with
/// the one `[Ljava.lang.String;` signature). INVOKE SHAPE
/// (live-proven on 8.3.6 / JDK 17): the params Object[] must wrap
/// the args String[] as its single element — passing the bare
/// String[] reads as "zero parameters" and the MBean answers
/// NoSuchMethodException ("mismatched signature … or parameters").
const JFR_PRELUDE: &str = r#"
import system

def _ign_jfr_diag():
    from java.lang.management import ManagementFactory
    from javax.management import ObjectName
    return ManagementFactory.getPlatformMBeanServer(), \
        ObjectName('com.sun.management:type=DiagnosticCommand')

def _ign_jfr_invoke(mbean, oname, op, args):
    from jarray import array
    from java.lang import String, Object
    return str(mbean.invoke(oname, op,
                            array([array(args, String)], Object),
                            array(['[Ljava.lang.String;'], String)))

def _ign_jfr_check(mbean, oname):
    return _ign_jfr_invoke(mbean, oname, 'jfrCheck', [])
"#;

/// `jfrStart` — memory-buffered (NO `filename=` at start: nothing
/// touches gateway disk until `stop` dumps), duration-capped so the
/// recording auto-stops even if the caller forgets. A name collision
/// (a leftover `ign-cli` recording) refuses with the stop command
/// named.
pub fn build_jfr_start_code(opts: &JvmJfrStartOptions) -> String {
    JFR_START_TEMPLATE
        .replace("__NAME__", JFR_RECORDING_NAME)
        .replace("__DURATION_SECS__", &opts.duration_secs.to_string())
        .replace("__SETTINGS__", opts.settings.as_str())
}

const JFR_START_TEMPLATE: &str = r#"
_mbean, _oname = _ign_jfr_diag()
_check = _ign_jfr_check(_mbean, _oname)
if 'name=__NAME__' in _check:
    raise Exception('a flight recording named __NAME__ already exists — stop it first: ign jvm jfr stop')
_out = _ign_jfr_invoke(_mbean, _oname, 'jfrStart',
    ['name=__NAME__', 'duration=__DURATION_SECS__s', 'settings=__SETTINGS__'])
_result = system.util.jsonEncode({
    'output': _out,
    'recordings': _ign_jfr_check(_mbean, _oname),
})
"#;

/// `jfrCheck` — the verbatim recording list plus whether OUR name is
/// present (the TUI polls this to keep its banner honest).
pub fn build_jfr_status_code() -> String {
    JFR_STATUS_TEMPLATE.replace("__NAME__", JFR_RECORDING_NAME)
}

const JFR_STATUS_TEMPLATE: &str = r#"
_mbean, _oname = _ign_jfr_diag()
_check = _ign_jfr_invoke(_mbean, _oname, 'jfrCheck', [])
_result = system.util.jsonEncode({
    'recordings': _check,
    'active': 'name=__NAME__' in _check,
})
"#;

/// `jfrDump` → size check → base64 stream-back → `jfrStop` → remote
/// delete. Over-cap dumps are NOT read: the recording is stopped, the
/// file left in place, and its path+size reported (`tooLarge`).
pub fn build_jfr_stop_code() -> String {
    JFR_STOP_TEMPLATE
        .replace("__NAME__", JFR_RECORDING_NAME)
        .replace("__MAX_BYTES__", &JFR_DUMP_MAX_BYTES.to_string())
}

const JFR_STOP_TEMPLATE: &str = r#"
import time as _t
from java.io import File
from java.lang import System
_mbean, _oname = _ign_jfr_diag()
_check = _ign_jfr_check(_mbean, _oname)
if 'name=__NAME__' not in _check:
    raise Exception('no flight recording named __NAME__ exists — start one first: ign jvm jfr start')
_path = System.getProperty('java.io.tmpdir') + '/__NAME__-' + str(int(_t.time())) + '.jfr'
_ign_jfr_invoke(_mbean, _oname, 'jfrDump', ['name=__NAME__', 'filename=' + _path])
_ign_jfr_invoke(_mbean, _oname, 'jfrStop', ['name=__NAME__'])
_f = File(_path)
_size = _f.length()
if _size > __MAX_BYTES__:
    _result = system.util.jsonEncode({
        'remoteFile': _path, 'bytes': _size,
        'tooLarge': True, 'data': None, 'deleted': False,
    })
else:
    # The dump holds JVM memory contents: a failed read/encode must
    # NOT strand it on the gateway (review finding — the old
    # read-then-delete sequence left the file with its path
    # unreported). Delete in the failure path and name the path in
    # the raised error as the last-resort breadcrumb.
    import base64
    try:
        _fh = open(_path, 'rb')
        try:
            _raw = _fh.read()
        finally:
            _fh.close()
        _b64 = base64.b64encode(_raw)
    except Exception as _e:
        _cleaned = _f.delete()
        raise Exception('jfr dump fetch failed at ' + _path +
            ' (gateway-side delete: ' + str(_cleaned) + '): ' + str(_e))
    _deleted = _f.delete()
    _result = system.util.jsonEncode({
        'remoteFile': _path, 'bytes': _size,
        'tooLarge': False, 'data': _b64,
        'deleted': bool(_deleted),
    })
"#;

// ── The scriptExec plumbing (the 07-03 shape + the timeout override) ──

/// Resolve the profile's persisted scriptExec secret (the structural
/// gate: no secret = the route was never deployed, refuse before ANY
/// HTTP — script.rs's must-have truth #3).
fn resolve_secret(config: &Config, profile_name: &str) -> Result<String, CoreError> {
    config
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.webdev_secret.clone())
        .ok_or_else(|| CoreError::ScriptExecNotConfigured {
            profile: profile_name.to_string(),
        })
}

/// The version handshake + one exec carrying `code`, with the
/// per-request timeout override on the exec (a sampling run holds the
/// connection for its whole duration — the 30 s client default would
/// kill it). Returns the exec envelope's `data` ({stdout, result,
/// elapsedMs}) — the secret appears in NONE of it.
async fn exec_with_timeout(
    api: &dyn GatewayApi,
    config: &Config,
    profile_name: &str,
    project: &str,
    code: &str,
    timeout: Duration,
) -> Result<serde_json::Value, CoreError> {
    let secret = resolve_secret(config, profile_name)?;
    // Precondition handshake (plain timeout — it answers instantly).
    api.webdev_route_call(
        project,
        SCRIPT_EXEC_ROUTE,
        &serde_json::json!({"action": "version"}),
        &[(SECRET_HEADER, secret.as_str())],
    )
    .await?;
    api.webdev_route_call_with_timeout(
        project,
        SCRIPT_EXEC_ROUTE,
        &serde_json::json!({"action": "exec", "code": code}),
        &[(SECRET_HEADER, secret.as_str())],
        timeout,
    )
    .await
}

/// Pull the payload out of an exec envelope: the Jython sets
/// `_result` to a jsonEncode STRING (the route walker never sees
/// raw Java/Jython containers — one serialization layer, parsed
/// here), so `data.result` is a string we parse as JSON.
fn parse_payload(data: &serde_json::Value, verb: &str) -> Result<serde_json::Value, CoreError> {
    let raw = data
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    serde_json::from_str(raw).map_err(|err| {
        CoreError::Internal(format!(
            "jvm {verb}: the gateway's payload is not the JSON the payload builder \
             emits (route/exec contract drift): {err}"
        ))
    })
}

/// The envelope's route-measured wall time (0 when absent — the key
/// still rides on the models).
fn elapsed_ms(data: &serde_json::Value) -> u64 {
    data.get("elapsedMs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

// ── The actions ────────────────────────────────────────────────────

/// `ign jvm sample` — validate (usage errors lead), gate (the risk
/// acceptance), THEN instrument. Validation precedes the gate so a
/// mistyped duration refuses with `invalid_input` even when the risk
/// flag is missing (the put convention: usage errors lead, exit 2,
/// profile null either way).
pub async fn jvm_sample(
    api: &dyn GatewayApi,
    config: &Config,
    profile_name: &str,
    project: &str,
    opts: &JvmSampleOptions,
    risk_accepted: bool,
) -> Result<JvmSampleResult, CoreError> {
    validate_sample_options(opts)?;
    require_risk_acceptance(risk_accepted, "jvm sample")?;
    let code = build_sample_code(opts);
    let timeout = Duration::from_secs(opts.duration_secs + EXEC_TIMEOUT_MARGIN_SECS);
    let data = exec_with_timeout(api, config, profile_name, project, &code, timeout).await?;
    let payload = parse_payload(&data, "sample")?;

    let samples = payload["samples"].as_u64().unwrap_or_default();
    let observations = payload["observations"].as_u64().unwrap_or_default();
    // Per-THREAD counts normalize against per-THREAD observations —
    // never against samples (live-proven: a busy gateway's hot rows
    // read 7000%+ under per-sample math).
    let pct = |count: u64| -> f64 {
        if observations == 0 {
            0.0
        } else {
            ((count as f64 / observations as f64) * 1000.0).round() / 10.0
        }
    };
    let hot_methods = payload["hot"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| HotMethod {
                    frame: row["frame"].as_str().unwrap_or_default().to_string(),
                    count: row["count"].as_u64().unwrap_or_default(),
                    pct: pct(row["count"].as_u64().unwrap_or_default()),
                })
                .collect()
        })
        .unwrap_or_default();
    let top_stacks = payload["stacks"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| HotStack {
                    stack: row["stack"].as_str().unwrap_or_default().to_string(),
                    count: row["count"].as_u64().unwrap_or_default(),
                    pct: pct(row["count"].as_u64().unwrap_or_default()),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(JvmSampleResult {
        warning: PRODUCTION_WARNING,
        duration_secs: opts.duration_secs,
        interval_ms: opts.interval_ms,
        samples,
        thread_peak: payload["threadPeak"].as_u64().unwrap_or_default(),
        observations,
        wall_ms: payload["wallMs"].as_u64().unwrap_or_default(),
        hot_methods,
        top_stacks,
        folded: payload["folded"].as_str().unwrap_or_default().to_string(),
        elapsed_ms: elapsed_ms(&data),
    })
}

/// `ign jvm jfr start` — validate, gate, start the capped recording.
pub async fn jvm_jfr_start(
    api: &dyn GatewayApi,
    config: &Config,
    profile_name: &str,
    project: &str,
    opts: &JvmJfrStartOptions,
    risk_accepted: bool,
) -> Result<JvmJfrStartResult, CoreError> {
    validate_jfr_start_options(opts)?;
    require_risk_acceptance(risk_accepted, "jvm jfr start")?;
    let code = format!("{JFR_PRELUDE}\n{}", build_jfr_start_code(opts));
    let data = exec_with_timeout(
        api,
        config,
        profile_name,
        project,
        &code,
        Duration::from_secs(30),
    )
    .await?;
    let payload = parse_payload(&data, "jfr start")?;
    Ok(JvmJfrStartResult {
        warning: PRODUCTION_WARNING,
        name: JFR_RECORDING_NAME,
        duration_secs: opts.duration_secs,
        settings: opts.settings.as_str(),
        detail: payload["output"].as_str().unwrap_or_default().to_string(),
        elapsed_ms: elapsed_ms(&data),
    })
}

/// `ign jvm jfr status` — READ-only (jfrCheck changes nothing), so NO
/// risk gate: checking whether a recording is running must always be
/// cheap to reach (it is how you confirm cleanup).
pub async fn jvm_jfr_status(
    api: &dyn GatewayApi,
    config: &Config,
    profile_name: &str,
    project: &str,
) -> Result<JvmJfrStatusResult, CoreError> {
    let code = format!("{JFR_PRELUDE}\n{}", build_jfr_status_code());
    let data = exec_with_timeout(
        api,
        config,
        profile_name,
        project,
        &code,
        Duration::from_secs(30),
    )
    .await?;
    let payload = parse_payload(&data, "jfr status")?;
    Ok(JvmJfrStatusResult {
        warning: PRODUCTION_WARNING,
        active: payload["active"].as_bool().unwrap_or_default(),
        recordings: payload["recordings"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        elapsed_ms: elapsed_ms(&data),
    })
}

/// `ign jvm jfr stop` — stop + dump + fetch + remote cleanup. NO risk
/// gate: stopping a recording REDUCES gateway risk and must never be
/// refused (a forgotten recording is the failure mode this verb
/// exists to end). The local file write is the only filesystem touch:
/// `output` or the `ign-cli-<unix_ts>.jfr` default (the
/// backup/bundle fallback-naming precedent).
pub async fn jvm_jfr_stop(
    api: &dyn GatewayApi,
    config: &Config,
    profile_name: &str,
    project: &str,
    output: Option<&Path>,
) -> Result<JvmJfrStopResult, CoreError> {
    let code = format!("{JFR_PRELUDE}\n{}", build_jfr_stop_code());
    // 32 MiB base64 through the envelope: give the transfer room.
    let data = exec_with_timeout(
        api,
        config,
        profile_name,
        project,
        &code,
        Duration::from_secs(120),
    )
    .await?;
    let payload = parse_payload(&data, "jfr stop")?;

    let bytes = payload["bytes"].as_u64().unwrap_or_default();
    let remote_file = payload["remoteFile"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let too_large = payload["tooLarge"].as_bool().unwrap_or_default();

    if too_large {
        return Ok(JvmJfrStopResult {
            warning: PRODUCTION_WARNING,
            name: JFR_RECORDING_NAME,
            file: None,
            fetched: false,
            bytes,
            remote_file,
            remote_deleted: false,
            elapsed_ms: elapsed_ms(&data),
        });
    }

    let encoded = payload["data"].as_str().unwrap_or_default();
    let decoded = BASE64.decode(encoded).map_err(|err| {
        CoreError::Internal(format!(
            "jvm jfr stop: the gateway's dump is not valid base64 (route/exec \
             contract drift): {err}"
        ))
    })?;
    let default_name;
    let path = match output {
        Some(path) => path.to_path_buf(),
        None => {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            default_name = format!("{JFR_RECORDING_NAME}-{ts}.jfr");
            std::path::PathBuf::from(&default_name)
        }
    };
    std::fs::write(&path, &decoded).map_err(|err| CoreError::InvalidInput {
        reason: format!("cannot write {}: {err}", path.display()),
    })?;

    Ok(JvmJfrStopResult {
        warning: PRODUCTION_WARNING,
        name: JFR_RECORDING_NAME,
        file: Some(path.display().to_string()),
        fetched: true,
        bytes,
        remote_file,
        remote_deleted: payload["deleted"].as_bool().unwrap_or_default(),
        elapsed_ms: elapsed_ms(&data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> JvmSampleOptions {
        JvmSampleOptions::default()
    }

    // ── The risk gate ──────────────────────────────────────────────

    /// The gate refuses without the acceptance, with the additive
    /// slug, exit 2, and a hint that names the production caution —
    /// and `--yes` structurally CANNOT satisfy it (the function's
    /// only input is the explicit acceptance).
    #[test]
    fn risk_gate_refuses_without_acceptance() {
        let err = require_risk_acceptance(false, "jvm sample").expect_err("ungated refuses");
        assert_eq!(err.code(), "profiling_risk_not_accepted");
        assert_eq!(err.exit_code(), 2);
        assert!(
            err.to_string().contains(RISK_FLAG),
            "the message names the flag: {err}"
        );
        let hint = err.hint().expect("hint required");
        assert!(
            hint.contains("production") || hint.contains("PRODUCTION"),
            "the hint names the production caution: {hint}"
        );
        assert!(
            hint.contains("--yes deliberately does NOT"),
            "the hint states --yes cannot satisfy it: {hint}"
        );
        assert!(require_risk_acceptance(true, "jvm sample").is_ok());
    }

    // ── Validation: refuse, never clamp ────────────────────────────

    #[test]
    fn sample_options_refuse_outside_the_caps() {
        let mut o = opts();
        o.duration_secs = SAMPLE_DURATION_MAX_SECS + 1;
        let err = validate_sample_options(&o).expect_err("above cap refuses");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("120"), "names the cap: {err}");

        let mut o = opts();
        o.duration_secs = SAMPLE_DURATION_MIN_SECS - 1;
        assert!(validate_sample_options(&o).is_err(), "below floor refuses");

        let mut o = opts();
        o.interval_ms = SAMPLE_INTERVAL_MIN_MS - 1;
        assert!(validate_sample_options(&o).is_err(), "interval floor");
        let mut o = opts();
        o.interval_ms = SAMPLE_INTERVAL_MAX_MS + 1;
        assert!(validate_sample_options(&o).is_err(), "interval cap");

        let mut o = opts();
        o.top = 0;
        assert!(validate_sample_options(&o).is_err(), "top floor");
        let mut o = opts();
        o.top = HOT_METHODS_MAX + 1;
        assert!(validate_sample_options(&o).is_err(), "top cap");

        // The boundaries themselves pass.
        let o = JvmSampleOptions {
            duration_secs: SAMPLE_DURATION_MAX_SECS,
            interval_ms: SAMPLE_INTERVAL_MIN_MS,
            top: HOT_METHODS_MAX,
        };
        assert!(validate_sample_options(&o).is_ok(), "boundaries pass");
    }

    #[test]
    fn jfr_options_refuse_outside_the_caps() {
        let o = JvmJfrStartOptions {
            duration_secs: JFR_DURATION_MAX_SECS + 1,
            ..JvmJfrStartOptions::default()
        };
        let err = validate_jfr_start_options(&o).expect_err("above cap refuses");
        assert!(err.to_string().contains("300"), "names the cap: {err}");
        let o = JvmJfrStartOptions {
            duration_secs: JFR_DURATION_MIN_SECS - 1,
            ..JvmJfrStartOptions::default()
        };
        assert!(validate_jfr_start_options(&o).is_err(), "below floor");
        let o = JvmJfrStartOptions {
            duration_secs: JFR_DURATION_MAX_SECS,
            settings: JfrSettings::Profile,
        };
        assert!(validate_jfr_start_options(&o).is_ok(), "boundaries pass");
    }

    // ── The payload builders ───────────────────────────────────────

    /// Every token is substituted (no `__` marker survives) and the
    /// substituted values are the validated ones — the substitution
    /// safety contract (validated ints + whitelisted tokens ONLY).
    #[test]
    fn builders_substitute_every_token() {
        let o = JvmSampleOptions {
            duration_secs: 45,
            interval_ms: 50,
            top: 10,
        };
        let code = build_sample_code(&o);
        for token in [
            "__DURATION_MS__",
            "__INTERVAL_MS__",
            "__TOP__",
            "__MAX_STACKS__",
            "__MAX_FRAMES__",
        ] {
            assert!(!code.contains(token), "unsubstituted {token}");
        }
        assert!(code.contains("45000"), "duration in ms");
        assert!(code.contains("50"), "interval");
        assert!(
            code.contains("Thread.currentThread().getName()"),
            "self-exclusion"
        );

        let code = build_jfr_start_code(&JvmJfrStartOptions {
            duration_secs: 90,
            settings: JfrSettings::Profile,
        });
        for token in ["__NAME__", "__DURATION_SECS__", "__SETTINGS__"] {
            assert!(!code.contains(token), "unsubstituted {token}");
        }
        assert!(code.contains("duration=90s"));
        assert!(code.contains("settings=profile"));
        assert!(code.contains("name=ign-cli"));

        let code = build_jfr_status_code();
        assert!(!code.contains("__NAME__"));
        assert!(code.contains("jfrCheck"));

        let code = build_jfr_stop_code();
        for token in ["__NAME__", "__MAX_BYTES__"] {
            assert!(!code.contains(token), "unsubstituted {token}");
        }
        assert!(code.contains(&JFR_DUMP_MAX_BYTES.to_string()));
        assert!(code.contains("jfrDump"));
        assert!(code.contains("jfrStop"));
        assert!(code.contains("base64.b64encode"));
        // Dump BEFORE stop (a stop discards an un-dumped memory recording).
        let dump = code.find("jfrDump").expect("dump present");
        let stop = code.find("jfrStop").expect("stop present");
        assert!(dump < stop, "dump precedes stop");
    }

    // ── Payload parsing ────────────────────────────────────────────

    #[test]
    fn parse_payload_reads_the_result_string() {
        let data = serde_json::json!({
            "stdout": "",
            "result": "{\"samples\": 3}",
            "elapsedMs": 7,
        });
        let payload = parse_payload(&data, "sample").expect("parses");
        assert_eq!(payload["samples"], 3);
        assert_eq!(elapsed_ms(&data), 7);
        // A non-string/garbage result is an honesty error, not a panic.
        let data = serde_json::json!({"result": 42});
        let err = parse_payload(&data, "sample").expect_err("garbage refuses");
        assert_eq!(err.code(), "internal");
    }
}
