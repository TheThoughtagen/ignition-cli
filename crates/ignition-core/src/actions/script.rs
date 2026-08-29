//! The script action (07-03, SCRPT-01) — `ign script run`, the
//! smallest verb in the CLI: one action over the already-shipped,
//! already-secured scriptExec route (05-01's template, 05-03's
//! deploy/secret lifecycle).
//!
//! The opt-in is STRUCTURAL, not a flag: scriptExec deploys ONLY via
//! `ign webdev deploy --with-script-exec`, whose deploy persists a
//! 32-byte hex secret in the profile config at 0600 BEFORE upload.
//! `script_run` resolves that secret FIRST — no stored secret means
//! the route was never deployed, and the verb refuses with the
//! additive `script_exec_not_configured` (exit 6) naming the deploy
//! flag verbatim. There is NO `--yes` guard on script run (the
//! research-adopted decision): the deploy flag IS the opt-in, the
//! verb is the route's entire purpose, and agents need it
//! non-interactive.
//!
//! Sequence per invocation (two round trips, the 05-04
//! precondition's correctness-over-latency precedent):
//!
//! 1. **Precondition handshake** — the version action WITH the
//!    secret header. The 200-BODY envelope is the only success
//!    oracle (WebDev ignores `status`; denials ride 200); a
//!    `secret_required`/`secret_mismatch` denial surfaces honestly
//!    through the existing webdev error family — a mismatch means
//!    the route was deployed elsewhere/stale, and the hint already
//!    says redeploy or `--rotate-secret`. No new slug, no
//!    version-compare magic (the tags precondition owns that
//!    discrimination for its family; scriptExec's gate IS the
//!    secret).
//! 2. **Exec** — `{"action": "exec", "code": <code>}` with the
//!    secret header. A route error body (`{ok:false,
//!    error{code,message,traceback?}}` at HTTP 200) maps through
//!    the same denial seam with the traceback surfaced (the 05-08
//!    pattern) — a route-side Python exception is a black box no
//!    more.
//!
//! Timeout honesty (planner decision): v1.0.0's route has NO
//! server-side timeout — the client rides the existing per-request
//! class, and a long-running script simply holds the HTTP connection
//! (README documents this).
//!
//! [`read_script_input`] is the PURE three-form input reader
//! (`--code STR`, `--file PATH`, `--file -` stdin — the agent pipe
//! path), unit-tested separately from the async action: usage-class
//! errors lead (exit 2, the 03-03 put convention).

use std::io::Read;

use serde::Serialize;

use crate::actions::webdev::{SCRIPT_EXEC_ROUTE, SECRET_HEADER};
use crate::client::GatewayApi;
use crate::config::Config;
use crate::error::CoreError;

/// `ign script run` result — the route's exec answer under
/// unit-explicit keys, ALL keys always (the family convention:
/// agents never key-hunt). The secret appears in NONE of them
/// (redaction — the 05-03 canary extended to this surface).
#[derive(Debug, Serialize)]
pub struct ScriptRunResult {
    /// The script's captured stdout, verbatim (empty string when the
    /// script printed nothing — the key still rides).
    pub stdout: String,
    /// The script's value: a single expression's eval result, or the
    /// `_result` global statements left (null when neither — the key
    /// still rides). Raw JSON passthrough, never interpreted.
    pub result: serde_json::Value,
    /// The route-measured wall time in milliseconds (route-side
    /// `time.time()` deltas; 0 when the answer carried none — the
    /// key still rides).
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
}

/// `ign script run` — resolve the profile's stored scriptExec
/// secret, prove the route answers it, execute the code.
///
/// Resolution order is fixed: the secret gate FIRST (None →
/// `script_exec_not_configured`, zero HTTP), then the version
/// handshake, then exec. The config is the caller's already-loaded
/// view (main.rs resolves the profile for the client anyway; the
/// TUI loads it inside the worker — the fire_webdev_status
/// precedent).
pub async fn script_run(
    api: &dyn GatewayApi,
    config: &Config,
    profile_name: &str,
    project: &str,
    code: &str,
) -> Result<ScriptRunResult, CoreError> {
    // THE structural gate: no persisted secret = the route was never
    // deployed through the opt-in flag. Refuses before ANY network
    // I/O (must-have truth #3: zero HTTP requests).
    let secret = config
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.webdev_secret.clone())
        .ok_or_else(|| CoreError::ScriptExecNotConfigured {
            profile: profile_name.to_string(),
        })?;

    // Precondition handshake: the version action WITH the secret.
    // Success = the 200-BODY ok envelope (the only oracle); a denial
    // body surfaces honestly through webdev_route_call's existing
    // mapping (secret_required/secret_mismatch → webdev_route_error
    // whose hint names redeploy or --rotate-secret).
    api.webdev_route_call(
        project,
        SCRIPT_EXEC_ROUTE,
        &serde_json::json!({"action": "version"}),
        &[(SECRET_HEADER, secret.as_str())],
    )
    .await?;

    // Exec: the code rides verbatim; the envelope's data carries
    // {stdout, result, elapsedMs} (the 05-01 route contract). A
    // denial (route error with traceback) maps at the same seam.
    let data = api
        .webdev_route_call(
            project,
            SCRIPT_EXEC_ROUTE,
            &serde_json::json!({"action": "exec", "code": code}),
            &[(SECRET_HEADER, secret.as_str())],
        )
        .await?;

    Ok(ScriptRunResult {
        stdout: data
            .get("stdout")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        result: data
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        elapsed_ms: data
            .get("elapsedMs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default(),
    })
}

/// The three-form input reader (PURE — no async, no gateway):
/// `--code STR` passes through, `--file PATH` reads the file,
/// `--file -` reads stdin (the agent pipe path). Both `--code` AND
/// `--file` given → InvalidInput (usage errors lead — the caller
/// runs this BEFORE profile resolution, exit 2, profile null);
/// neither given → InvalidInput; unreadable file/stdin →
/// InvalidInput (the 03-03 put convention: the reason names the
/// source).
pub fn read_script_input(code: Option<&str>, file: Option<&str>) -> Result<String, CoreError> {
    match (code, file) {
        (Some(_), Some(_)) => Err(CoreError::InvalidInput {
            reason: "provide exactly one of --code or --file (not both)".to_string(),
        }),
        (Some(code), None) => Ok(code.to_string()),
        (None, Some("-")) => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|err| CoreError::InvalidInput {
                    reason: format!("cannot read stdin: {err}"),
                })?;
            Ok(buffer)
        }
        (None, Some(file)) => {
            std::fs::read_to_string(file).map_err(|err| CoreError::InvalidInput {
                reason: format!("cannot read {file}: {err}"),
            })
        }
        (None, None) => Err(CoreError::InvalidInput {
            reason: "provide the script via --code PY or --file PATH (--file - reads stdin)"
                .to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{read_script_input, script_run};
    use crate::client::GatewayApi;
    use crate::config;
    use crate::error::CoreError;
    use std::path::PathBuf;

    /// The recorded (action, body) sequence the rig hands back for
    /// assertion (an alias keeps the helper's signature legible).
    type CallLog = std::sync::Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>;

    /// A scripted double over the ONE call script_run makes —
    /// `webdev_route_call` answers from a lookup keyed on the body's
    /// action token, recorded through a Mutex so the closure stays
    /// `Fn` (the webdev.rs double shape). Everything else is
    /// unreachable.
    struct ScriptRig {
        calls: CallLog,
        answers: fn(&str) -> Result<serde_json::Value, CoreError>,
    }

    #[async_trait::async_trait]
    impl GatewayApi for ScriptRig {
        async fn tag_provider_list(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::tags::TagProviderRecord>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn tag_provider_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::tags::TagProviderRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_create(
            &self,
            _body: &[crate::client::tags::TagProviderCreate],
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_delete(
            &self,
            _name: &str,
            _signature: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_call(
            &self,
            _project: &str,
            _route: &str,
            body: &serde_json::Value,
            _extra_headers: &[(&str, &str)],
        ) -> Result<serde_json::Value, CoreError> {
            let action = body["action"].as_str().unwrap_or_default().to_string();
            self.calls
                .lock()
                .expect("calls lock")
                .push((action.clone(), body.clone()));
            (self.answers)(&action)
        }
        async fn gateway_info(&self) -> Result<crate::client::version::GatewayInfo, CoreError> {
            unreachable!("not part of this action")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unreachable!("not part of this action")
        }
        async fn status_ping(&self) -> Result<crate::client::status::StatusPing, CoreError> {
            unreachable!("not part of this action")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::status::ModuleInfo>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn metrics_current(
            &self,
        ) -> Result<crate::client::metrics::CurrentGauges, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_historic(
            &self,
        ) -> Result<crate::client::metrics::PerformanceCharts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_threads(&self) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn designers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::DesignerInfo>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::PerspectiveSession>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::VisionClient>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn database_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LogEntry>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LoggerInfo>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn restart(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn scan_projects(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn security_properties(
            &self,
        ) -> Result<crate::client::restart::SecurityProperties, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_status(&self, _route: &str) -> Result<u16, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_probe(
            &self,
            _project: &str,
            _route: &str,
            _extra_headers: &[(&str, &str)],
        ) -> Result<crate::client::webdev::RouteProbe, CoreError> {
            unreachable!("not part of this action")
        }
        async fn projects(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::projects::ProjectRecord>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn project_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::projects::ProjectRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_create(
            &self,
            _body: &crate::client::projects::ProjectCreate,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_copy(&self, _from: &str, _to: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_rename(&self, _name: &str, _new_name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_modify(
            &self,
            _name: &str,
            _body: &crate::client::projects::ProjectModify,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_delete(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_export_to_file(
            &self,
            _name: &str,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_import(
            &self,
            _name: &str,
            _zip: Vec<u8>,
            _overwrite: bool,
        ) -> Result<crate::client::projects::ImportOutcome, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_download(
            &self,
            _out: &std::path::Path,
            _backup_type: crate::client::backup::BackupType,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_restore(&self, _gwbk: &std::path::Path) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_history(
            &self,
            _limit: Option<u32>,
            _search: Option<&str>,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamHistoryItem>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_definitions(
            &self,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamTaskRecord>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::eam::EamTaskRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_create(&self, _definition: &serde_json::Value) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_force(&self, _owner: &str, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
    }

    /// Temp config with one `dev` profile; the optional secret seeds
    /// `webdev_secret` (the persisted-secret gate's two states).
    fn temp_config(secret: Option<&str>) -> (tempfile::TempDir, config::Config, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        let secret_line = secret
            .map(|secret| format!("webdev_secret = \"{secret}\"\n"))
            .unwrap_or_default();
        std::fs::write(
            &path,
            format!("active = \"dev\"\n\n[profiles.dev]\nurl = \"http://localhost:9088/\"\n{secret_line}"),
        )
        .expect("write config");
        let config = config::load(&path).expect("config loads");
        (dir, config, path)
    }

    /// A rig whose every call answers from a table keyed on the
    /// action token, recording the bodies it saw.
    fn rig(answers: fn(&str) -> Result<serde_json::Value, CoreError>) -> (ScriptRig, CallLog) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            ScriptRig {
                calls: std::sync::Arc::clone(&calls),
                answers,
            },
            calls,
        )
    }

    /// THE structural gate: no stored secret → the additive slug,
    /// exit 6, hint naming the deploy flag — and ZERO route calls.
    #[tokio::test]
    async fn missing_secret_refuses_before_any_call() {
        let (_dir, config, _path) = temp_config(None);
        let (double, calls) = rig(|_| unreachable!("the gate refuses before any call"));
        let err = script_run(&double, &config, "dev", "ign-cli", "2+2")
            .await
            .expect_err("no secret refuses");
        assert_eq!(err.code(), "script_exec_not_configured");
        assert_eq!(err.exit_code(), 6);
        assert!(
            err.hint()
                .unwrap()
                .contains("ign webdev deploy --with-script-exec"),
            "hint names the deploy flag: {:?}",
            err.hint()
        );
        assert!(
            calls.lock().expect("calls lock").is_empty(),
            "zero route calls"
        );
    }

    /// The success round: version probe then exec, both seen, the
    /// exec body carrying the code verbatim, and the answer mapped
    /// under {stdout, result, elapsedMs} with ALL keys always.
    #[tokio::test]
    async fn success_round_probes_then_execs_and_maps_the_envelope() {
        let (_dir, config, _path) = temp_config(Some("aabbcc"));
        let (double, calls) = rig(|action| {
            Ok(match action {
                "version" => serde_json::json!({"routeVersion": "1.0.0", "minCli": "1.0"}),
                _ => serde_json::json!({
                    "stdout": "hello\n",
                    "result": 4,
                    "elapsedMs": 12,
                }),
            })
        });
        let result = script_run(&double, &config, "dev", "ign-cli", "print 'hello'\n2+2")
            .await
            .expect("exec succeeds");
        assert_eq!(result.stdout, "hello\n");
        assert_eq!(result.result, serde_json::json!(4));
        assert_eq!(result.elapsed_ms, 12);

        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 2, "exactly probe + exec");
        assert_eq!(calls[0].0, "version");
        assert_eq!(calls[1].0, "exec");
        assert_eq!(
            calls[1].1["code"], "print 'hello'\n2+2",
            "code rides verbatim"
        );

        // Serialized shape: unit-explicit keys, ALL always.
        let serialized = serde_json::to_value(&result).expect("serializes");
        assert_eq!(serialized["stdout"], "hello\n");
        assert_eq!(serialized["result"], 4);
        assert_eq!(serialized["elapsedMs"], 12);
        let mut keys: Vec<&str> = serialized
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["elapsedMs", "result", "stdout"]);
    }

    /// The missing-answer degrade: absent fields default (empty
    /// stdout, null result, 0 ms) instead of erroring — ALL keys
    /// still ride (the family convention).
    #[tokio::test]
    async fn absent_answer_fields_default_but_keys_ride() {
        let (_dir, config, _path) = temp_config(Some("aabbcc"));
        let (double, _calls) = rig(|_| Ok(serde_json::json!({})));
        let result = script_run(&double, &config, "dev", "ign-cli", "pass")
            .await
            .expect("an empty object still answers");
        assert_eq!(result.stdout, "");
        assert_eq!(result.result, serde_json::Value::Null);
        assert_eq!(result.elapsed_ms, 0);
    }

    /// A probe denial surfaces HONESTLY through the existing family
    /// (the rig hands back the error webdev_route_call would have
    /// mapped) — and exec NEVER fires.
    #[tokio::test]
    async fn probe_denial_surfaces_honestly_without_exec() {
        let (_dir, config, _path) = temp_config(Some("stale"));
        let (double, calls) = rig(|action| match action {
            "version" => Err(CoreError::WebdevRouteError {
                code: "secret_mismatch".to_string(),
                message: "scriptExec secret mismatch".to_string(),
                endpoint: Some("/system/webdev/ign-cli/cli/scriptExec".to_string()),
            }),
            _ => unreachable!("exec must not fire after a probe denial"),
        });
        let err = script_run(&double, &config, "dev", "ign-cli", "2+2")
            .await
            .expect_err("mismatch refuses");
        assert_eq!(err.code(), "webdev_route_error");
        assert_eq!(err.exit_code(), 6);
        assert!(
            err.hint().unwrap().contains("--rotate-secret"),
            "the existing hint carries the redeploy/rotate advice: {:?}",
            err.hint()
        );
        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 1, "only the probe ran");
        assert_eq!(calls[0].0, "version");
    }

    /// The pure three-form reader: --code passes through, --file
    /// reads disk, both/none/unreadable refuse InvalidInput (usage
    /// errors lead — the 03-03 put convention).
    #[test]
    fn read_script_input_resolves_the_three_forms() {
        // --code verbatim.
        assert_eq!(read_script_input(Some("2+2"), None).expect("code"), "2+2");
        // --file PATH reads the file.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("snippet.py");
        std::fs::write(&file, "print 'hi'\n").expect("write snippet");
        assert_eq!(
            read_script_input(None, file.to_str()).expect("file"),
            "print 'hi'\n"
        );
        // Both → InvalidInput naming the exclusivity.
        let err = read_script_input(Some("2+2"), file.to_str()).expect_err("both refuse");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
        assert!(
            err.to_string().contains("--code") && err.to_string().contains("--file"),
            "reason names both flags: {err}"
        );
        // Neither → InvalidInput naming the forms.
        let err = read_script_input(None, None).expect_err("neither refuses");
        assert_eq!(err.code(), "invalid_input");
        assert!(err.to_string().contains("--file -"), "stdin named: {err}");
        // Unreadable file → InvalidInput naming the path.
        let err = read_script_input(None, Some("/nonexistent/snippet.py")).expect_err("miss");
        assert_eq!(err.code(), "invalid_input");
        assert!(
            err.to_string().contains("/nonexistent/snippet.py"),
            "reason names the file: {err}"
        );
    }
}
