//! The WebDev client seam (05-03) — the CLI's own gateway-side
//! surface at `/system/webdev/{project}/cli/{route}` (NOT
//! `/data/webdev/*` — that prefix does not exist; 05-RESEARCH wire
//! protocol, live-proven on 8.3.3).
//!
//! This module owns the seam's PURE pieces so the trait impl in
//! [`super`] stays thin delegation (the per-capability-file
//! convention): the ONE path builder ([`route_url`]), the
//! presence/version discrimination enum ([`RouteProbe`]), the
//! shared 200-body envelope parser ([`parse_route_body`]) and its
//! taxonomy mapping ([`denial_to_error`]), and the deploy zip
//! builder ([`build_deploy_zip`]).
//!
//! THE wire rules pinned here (05-RESEARCH, all live-proven):
//! - **405 = absent, NOT 404** — missing routes AND missing projects
//!   both answer 405 (Pitfall 1; doctor's Phase-2 404 assumption was
//!   wrong and 05-03 re-pins it).
//! - **402 = module unlicensed** — a trial-expired gateway's WebDev
//!   servlet answers 402 with an HTML page (cross-verified 8.3.6).
//! - **Denials ride HTTP 200** — WebDev IGNORES a `status` key in
//!   route returns; every refusal is detectable only from the body
//!   envelope `{ok, data|error}`. The status code alone is NEVER a
//!   success verdict (Pitfall 2).
//!
//! [`build_deploy_zip`] packs the embedded 05-01 bundle
//! ([`crate::webdev`]) into the project-zip the deploy action
//! uploads through the 03-02 import machinery — scriptExec ONLY with
//! a SUBSTITUTED secret (fail closed: the template's placeholder
//! must never ship, the 05-01 structural guarantee enforced here at
//! the type level).

use std::io::Write;

use serde_json::Value;

use crate::error::CoreError;
use crate::webdev as bundle;

/// The deploy project the CLI owns wholesale — born from the first
/// deploy zip, overwrite-replaced by every later deploy (05-RESEARCH
/// deploy guidance; `--project` overrides it deliberately).
pub const DEFAULT_PROJECT: &str = "ign-cli";

/// The scriptExec route folder's zip root (the template's static
/// siblings — 05-01 embedded only the doPost.py TEMPLATE in
/// `crate::webdev`, so the two gate files embed HERE, at the seam
/// that packs them).
const SCRIPT_EXEC_ROUTE_ROOT: &str = "com.inductiveautomation.webdev/resources/cli/scriptExec";
const SCRIPT_EXEC_RESOURCE_JSON: &str = include_str!(
    "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/scriptExec/resource.json"
);
const SCRIPT_EXEC_CONFIG_JSON: &str = include_str!(
    "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/scriptExec/config.json"
);

/// The always-on route folders, in manifest order — DERIVED from
/// [`bundle::ROUTE_FILES`] so the deploy set, the status sweep, and
/// the manifest itself can never drift apart.
pub fn always_on_routes() -> Vec<String> {
    let mut routes = Vec::new();
    for (name, _) in bundle::ROUTE_FILES {
        let Some(rest) = name.strip_prefix("com.inductiveautomation.webdev/resources/cli/") else {
            continue;
        };
        if let Some((route, file)) = rest.rsplit_once('/')
            && file == "doPost.py"
            && !routes.iter().any(|known: &String| known == route)
        {
            routes.push(route.to_string());
        }
    }
    routes
}

/// Path builder: `/system/webdev/{project}/cli/{route}` — the `cli/`
/// folder segment is PART of the route folder path (the wire
/// protocol's URL shape; the `cli` folder groups the CLI's routes
/// inside the deploy project).
pub(crate) fn route_url(project: &str, route: &str) -> String {
    format!("/system/webdev/{project}/cli/{route}")
}

/// The presence/version discrimination — the probe enum. The status
/// code IS the answer (deliberately NOT run through classify, the
/// [`super::GatewayApi::webdev_route_status`] precedent); only
/// transport failures are errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteProbe {
    /// 200 + ok body from the version action: deployed and answering
    /// its handshake `routeVersion`.
    Present {
        /// The route's `routeVersion` handshake answer.
        route_version: String,
    },
    /// 405 — the live-proven 8.3 absent marker (missing route or
    /// missing project; NOT 404).
    Absent,
    /// 402 — the WebDev module is installed but unlicensed
    /// (trial-expired gateway).
    Unlicensed,
    /// 401/403 — something answers at the path but rejects the
    /// credential: present but auth-gated (research Open Question 4's
    /// resolution: report, never conflate with absent).
    AuthGated,
    /// 200 body denial (`{ok:false, error{code,message}}`) — present
    /// and refusing: the scriptExec gate's `secret_required` /
    /// `secret_mismatch`, or any other stable route-contract code.
    Denied {
        /// The route's machine error code (05-01 contract).
        code: String,
        /// The route's human message.
        message: String,
        /// The route's Python traceback, when the denial carries one
        /// (the envelope's optional `error.traceback` — surfaced so
        /// route-side exceptions are not a black box, 05-08).
        traceback: Option<String>,
    },
}

/// The 200-body verdict shared by [`super::GatewayApi::webdev_route_call`]
/// and [`super::GatewayApi::webdev_route_probe`].
#[derive(Debug)]
pub(crate) enum RouteBody {
    /// `ok:true` — `data` (Null when the route sent none).
    Ok(Value),
    /// `ok:false` — the route's structured refusal (traceback when
    /// the envelope carried one).
    Denied {
        code: String,
        message: String,
        traceback: Option<String>,
    },
}

/// Parse a 200 body as the route envelope. A body that is not the
/// `{ok, data|error}` shape is an internal-class honesty error — the
/// CLI's own routes ALWAYS answer the envelope, so anything else
/// means the path is not ours (a foreign route or an HTML error page
/// that smuggled past the status line). Missing `error` fields
/// degrade to the route contract's generic `route_error` code rather
/// than guessing.
pub(crate) fn parse_route_body(body: &str) -> Result<RouteBody, CoreError> {
    let value: Value = serde_json::from_str(body).map_err(|err| {
        CoreError::Internal(format!(
            "webdev route answered a body that is not the {{ok, data|error}} envelope: {err}"
        ))
    })?;
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(RouteBody::Ok(
            value.get("data").cloned().unwrap_or(Value::Null),
        ))
    } else {
        let code = value
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("route_error")
            .to_string();
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("(the route sent no message)")
            .to_string();
        let traceback = value
            .pointer("/error/traceback")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(RouteBody::Denied {
            code,
            message,
            traceback,
        })
    }
}

/// Map a body denial onto the taxonomy: the route contract's
/// `not_found` code reuses the existing [`CoreError::NotFound`] slug
/// (it means exactly that — the named thing is absent); the alarms
/// route's `no_alarm_journal` maps to the actionable
/// [`CoreError::AlarmJournalMissing`] (default rigs ALWAYS deny
/// history there — the missing journal chain is target state, not a
/// route bug, 05-06); every other code — known-but-unmapped like
/// `secret_required`, or unknown from a future route — rides
/// [`CoreError::WebdevRouteError`] with code + message verbatim, the
/// stable contract agents branch on. A denial that carried a
/// traceback gets it appended to the message
/// (`"\nroute traceback: {tb}"`) — the route-side exception is
/// visible instead of a black box (05-08); without one the message
/// is byte-identical to the pre-traceback era.
pub(crate) fn denial_to_error(
    code: &str,
    message: &str,
    traceback: Option<&str>,
    endpoint: String,
) -> CoreError {
    match code {
        "not_found" => CoreError::NotFound {
            endpoint: Some(endpoint),
        },
        "no_alarm_journal" => CoreError::AlarmJournalMissing {
            endpoint: Some(endpoint),
        },
        // The tagConfig route's provider-root refusal (07-06): the
        // route detects the bracket form pre-call and translates the
        // bare form's 'No RpcContext' throw — the honest
        // platform-limitation slug over a generic route error (the
        // no_alarm_journal seam precedent).
        "provider_root_unsupported" => CoreError::ProviderRootUnsupported {
            endpoint: Some(endpoint),
        },
        _ => {
            let mut full = message.to_string();
            if let Some(traceback) = traceback {
                full.push_str("\nroute traceback: ");
                full.push_str(traceback);
            }
            CoreError::WebdevRouteError {
                code: code.to_string(),
                message: full,
                endpoint: Some(endpoint),
            }
        }
    }
}

/// Pack the deploy zip: the embedded always-on bundle VERBATIM (the
/// project title substituted into `project.json`'s `title` ONLY when
/// `project_title` differs from [`DEFAULT_PROJECT`] — the manifest
/// already says `ign-cli`), plus — when `with_script_exec` —
/// scriptExec's three members with the secret SUBSTITUTED into the
/// template's `__IGN_CLI_SECRET__` marker (exactly-once replace; the
/// 05-01 contract test pins the marker count).
///
/// FAIL CLOSED: `with_script_exec` + `None` secret is an internal
/// bug guard (the deploy action generates the secret BEFORE packing;
/// shipping the unsubstituted template would arm the gate with the
/// publicly-known placeholder). `Some` WITHOUT `with_script_exec` is
/// tolerated and ignored — a stored profile secret never forces a
/// scriptExec deploy.
///
/// Members ride fixed `SimpleFileOptions` + deflate (the 05-02
/// deterministic-zip convention) so identical inputs pack
/// identically.
pub fn build_deploy_zip(
    project_title: &str,
    with_script_exec: bool,
    secret: Option<&str>,
) -> Result<Vec<u8>, CoreError> {
    let script_exec_py = match (with_script_exec, secret) {
        (false, _) => None,
        (true, Some(secret)) => {
            Some(bundle::SCRIPT_EXEC_TEMPLATE.replace("__IGN_CLI_SECRET__", secret))
        }
        (true, None) => {
            return Err(CoreError::Internal(
                "scriptExec deploy requires a substituted secret — the deploy \
                 action generates the secret before packing (fail-closed guard)"
                    .into(),
            ));
        }
    };

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for (name, contents) in bundle::ROUTE_FILES {
        let body = if *name == "project.json" && project_title != DEFAULT_PROJECT {
            retitle_project_json(contents, project_title)?
        } else {
            (*contents).to_string()
        };
        writer.start_file(*name, options).map_err(zip_write_err)?;
        writer.write_all(body.as_bytes()).map_err(|err| {
            CoreError::Internal(format!("cannot build the webdev deploy zip: {err}"))
        })?;
    }

    if let Some(script_exec_py) = script_exec_py {
        for (name, body) in [
            (
                format!("{SCRIPT_EXEC_ROUTE_ROOT}/resource.json"),
                SCRIPT_EXEC_RESOURCE_JSON.to_string(),
            ),
            (
                format!("{SCRIPT_EXEC_ROUTE_ROOT}/config.json"),
                SCRIPT_EXEC_CONFIG_JSON.to_string(),
            ),
            (
                format!("{SCRIPT_EXEC_ROUTE_ROOT}/doPost.py"),
                script_exec_py,
            ),
        ] {
            writer
                .start_file(name.as_str(), options)
                .map_err(zip_write_err)?;
            writer.write_all(body.as_bytes()).map_err(|err| {
                CoreError::Internal(format!("cannot build the webdev deploy zip: {err}"))
            })?;
        }
    }

    writer
        .finish()
        .map_err(zip_write_err)
        .map(|cursor| cursor.into_inner())
}

fn zip_write_err(err: zip::result::ZipError) -> CoreError {
    CoreError::Internal(format!("cannot build the webdev deploy zip: {err}"))
}

/// Swap `project.json`'s `title` for a `--project` override — only
/// `title` moves (name/description/enabled/parent ride verbatim; the
/// import NAME is the URL's concern, not the manifest's).
fn retitle_project_json(project_json: &str, title: &str) -> Result<String, CoreError> {
    let mut value: Value = serde_json::from_str(project_json).map_err(|err| {
        CoreError::Internal(format!("embedded project.json does not parse: {err}"))
    })?;
    value["title"] = Value::String(title.to_string());
    serde_json::to_string(&value)
        .map_err(|err| CoreError::Internal(format!("cannot re-serialize project.json: {err}")))
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_PROJECT, RouteBody, always_on_routes, build_deploy_zip, denial_to_error,
        parse_route_body,
    };
    use crate::error::CoreError;

    /// The route list is DERIVED from the manifest — the four
    /// always-on folders, manifest order, no drift possible.
    #[test]
    fn always_on_routes_derive_from_the_manifest() {
        assert_eq!(
            always_on_routes(),
            vec![
                "tags".to_string(),
                "tagConfig".to_string(),
                "alarms".to_string(),
                "tagHistory".to_string(),
            ]
        );
    }

    /// Envelope parsing: ok:true yields data; ok:false yields the
    /// code+message; a non-envelope body is internal-class; missing
    /// error fields degrade to the generic contract code.
    #[test]
    fn parse_route_body_envelope_shapes() {
        match parse_route_body(r#"{"ok":true,"data":{"routeVersion":"1.0.0"}}"#)
            .expect("ok body parses")
        {
            RouteBody::Ok(data) => {
                assert_eq!(data["routeVersion"], "1.0.0");
            }
            other => panic!("wrong verdict: {other:?}"),
        }

        match parse_route_body(
            r#"{"ok":false,"error":{"code":"secret_mismatch","message":"nope"}}"#,
        )
        .expect("denial parses")
        {
            RouteBody::Denied {
                code,
                message,
                traceback,
            } => {
                assert_eq!(code, "secret_mismatch");
                assert_eq!(message, "nope");
                assert!(traceback.is_none(), "no traceback on the wire");
            }
            other => panic!("wrong verdict: {other:?}"),
        }

        // A denial carrying the optional traceback keeps it (the
        // black-box fix's parse half).
        match parse_route_body(
            r#"{"ok":false,"error":{"code":"route_error","message":"boom","traceback":"Traceback (most recent call last):\n  ValueError: nope"}}"#,
        )
        .expect("denial with traceback parses")
        {
            RouteBody::Denied { code, traceback, .. } => {
                assert_eq!(code, "route_error");
                assert_eq!(
                    traceback.as_deref(),
                    Some("Traceback (most recent call last):\n  ValueError: nope")
                );
            }
            other => panic!("wrong verdict: {other:?}"),
        }

        // ok:true without data → Null (routes may answer bare oks).
        match parse_route_body(r#"{"ok":true}"#).expect("bare ok parses") {
            RouteBody::Ok(data) => assert!(data.is_null()),
            other => panic!("wrong verdict: {other:?}"),
        }

        // ok:false without an error object → the generic code, never
        // a guess.
        match parse_route_body(r#"{"ok":false}"#).expect("bare denial parses") {
            RouteBody::Denied { code, .. } => assert_eq!(code, "route_error"),
            other => panic!("wrong verdict: {other:?}"),
        }

        let err = parse_route_body("<html>jetty</html>").expect_err("non-envelope fails");
        assert!(matches!(err, CoreError::Internal(_)), "{err}");
    }

    /// The taxonomy mapping: `not_found` reuses the existing slug;
    /// everything else (known secret codes included) rides
    /// `webdev_route_error` verbatim; a traceback appends to the
    /// message (`\nroute traceback: {tb}`) while its absence keeps
    /// the message byte-identical.
    #[test]
    fn denial_mapping_reuses_not_found_and_rides_the_rest() {
        let not_found = denial_to_error("not_found", "no such path", None, "/x".into());
        assert_eq!(not_found.code(), "not_found");
        assert_eq!(not_found.exit_code(), 6);

        let secret = denial_to_error("secret_required", "missing header", None, "/x".into());
        assert_eq!(secret.code(), "webdev_route_error");
        assert_eq!(secret.exit_code(), 6);
        assert!(secret.to_string().contains("secret_required"));
        assert!(
            secret.to_string().contains("missing header"),
            "no traceback → the message rides VERBATIM (no suffix)"
        );

        // THE black-box fix: a denial with a traceback shows it —
        // the "Invalid UUID string" class is diagnosable from CLI
        // output alone.
        let blown = denial_to_error(
            "route_error",
            "error processing action",
            Some("java.lang.IllegalArgumentException: Invalid UUID string: 3f2504e0"),
            "/x".into(),
        );
        assert_eq!(blown.code(), "webdev_route_error");
        let text = blown.to_string();
        assert!(
            text.contains("\nroute traceback: java.lang.IllegalArgumentException: Invalid UUID string: 3f2504e0"),
            "the traceback rides the message: {text}"
        );

        // The alarms route's journal-missing denial maps to the
        // actionable slug (default rigs ALWAYS hit it — the missing
        // journal chain is target state, not a route bug).
        let journal = denial_to_error(
            "no_alarm_journal",
            "No alarm journal profile specified",
            None,
            "/system/webdev/ign-cli/cli/alarms".into(),
        );
        assert_eq!(journal.code(), "alarm_journal_missing");
        assert_eq!(journal.exit_code(), 6);
        assert!(
            journal.hint().unwrap().contains("journal profile"),
            "hint names the chain: {journal}"
        );

        // The tagConfig route's provider-root denial (07-06): the
        // dedicated slug + exit 6, the message names the subtree
        // workaround (the fixed Display — the route's own message
        // stays consistent with it).
        let root = denial_to_error(
            "provider_root_unsupported",
            "provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder",
            None,
            "/system/webdev/ign-cli/cli/tagConfig".into(),
        );
        assert_eq!(root.code(), "provider_root_unsupported");
        assert_eq!(root.exit_code(), 6);
        assert!(
            root.to_string().contains("subtree like [provider]folder"),
            "the fixed Display names the subtree workaround: {root}"
        );
    }

    /// THE fail-closed guard: scriptExec packing demands a secret —
    /// `None` + `with_script_exec` refuses BEFORE any zip is built.
    #[test]
    fn deploy_zip_fails_closed_without_a_script_exec_secret() {
        let err = build_deploy_zip(DEFAULT_PROJECT, true, None).expect_err("must refuse");
        assert!(matches!(err, CoreError::Internal(_)), "{err}");
        assert_eq!(err.exit_code(), 1);
    }
}
