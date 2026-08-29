//! The response classifier — the ONE place that sees status,
//! content-type, and redirect Location BEFORE any `.json()` call
//! (02-RESEARCH §Error-Body Sniffing). Every pipeline helper in [`super`]
//! routes its responses through [`classify`]; no response body is ever
//! parsed before its status has been mapped into the LOCKED taxonomy.
//!
//! Dispatch order (prescriptive, from research):
//! 1. **2xx** → pass the response through for body parsing.
//! 2. **3xx** → `Location` containing `/welcome` means the gateway is
//!    uncommissioned (it 302s EVERYTHING at the wizard); any other 3xx
//!    (e.g. `/idp/…` on `/data/app/*`) is an auth-class redirect.
//! 3. **401/403** → `Auth` (exit 5) — the status-aware hints in
//!    `CoreError::hint()` carry the name:key / three-parts guidance.
//! 4. **503** → `GatewayRestarting` (exit 6) — the webserver answers 503
//!    while the gateway restarts (verified lifecycle; never a decode error).
//! 5. **404** → `NotFound` (exit 6) — missing resource or a pre-8.3
//!    gateway's `No route match` JSON.
//! 6. Anything else → `Internal`, enriched with the Jetty HTML page's
//!    own title/message when the body is HTML (see [`html_error_parts`]).
//!
//! The Jetty sniffer is a deliberate substring scan, not an HTML crate:
//! the error pages are a fixed server template (research Don't-Hand-Roll).

use serde_json::Value;

use crate::error::CoreError;

/// Classify `resp` (from `url`) into `Ok(response)` on 2xx or the typed
/// [`CoreError`] every other observed gateway shape maps to. Consumes the
/// body ONLY on the unclassifiable fallback (to sniff the HTML detail);
/// classified variants keep their fixed Display strings.
pub(crate) async fn classify(
    resp: reqwest::Response,
    url: &str,
) -> Result<reqwest::Response, CoreError> {
    use reqwest::StatusCode as S;

    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }

    // Redirects: an uncommissioned gateway 302s everything to /welcome
    // (reqwest is configured with Policy::none() so we SEE the 3xx);
    // other redirect targets (e.g. /idp on /data/app/*) are auth-class.
    if status.is_redirection() {
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if location.contains("/welcome") {
            return Err(CoreError::GatewayNotCommissioned {
                endpoint: Some(url.to_string()),
            });
        }
        return Err(CoreError::Auth {
            status: status.as_u16(),
            endpoint: Some(url.to_string()),
        });
    }

    match status {
        // The EAM controller state gate (07-02, the trial_not_expired
        // pattern's classify edition): a 403 on a /data/eam/ path
        // whose body carries the controller message is a STATE
        // refusal — the token is fine, the module's role is not
        // (live-proven 8.3.3: every /data/eam/api/v1/* endpoint
        // answers exactly this). Path-scoped + content-scoped so a
        // generic under-permitted 403 elsewhere (or on an EAM path
        // with a different message) keeps the honest Auth mapping.
        S::FORBIDDEN if is_eam_url(url) => {
            let body = resp.text().await.unwrap_or_default();
            if body.contains("configured as a controller") {
                return Err(CoreError::EamNotController {
                    endpoint: Some(url.to_string()),
                });
            }
            Err(CoreError::Auth {
                status: status.as_u16(),
                endpoint: Some(url.to_string()),
            })
        }
        S::UNAUTHORIZED | S::FORBIDDEN => Err(CoreError::Auth {
            status: status.as_u16(),
            endpoint: Some(url.to_string()),
        }),
        S::SERVICE_UNAVAILABLE => Err(CoreError::GatewayRestarting {
            endpoint: Some(url.to_string()),
        }),
        S::NOT_FOUND => Err(CoreError::NotFound {
            endpoint: Some(url.to_string()),
        }),
        // 409 on the DESIGNER-PRUNE route only — route-scoped via the
        // URL (the singular prune path `/data/api/v1/designer/{id}` is
        // distinct from the plural `/designers` list; see
        // [`is_designer_prune_url`]). A LIVE Designer session answers
        // the prune DELETE with 409 + empty body (wire-verified 8.3.3,
        // 06-UAT test 6): prune removes STALE entries only, a
        // target-state refusal — not an internal error. Every other
        // route's 409 keeps the Internal fallback below.
        //
        // The Perspective terminate 404 ("No valid sessions found to
        // close" — the id-vs-scope mismatch of a Designer-embedded
        // session) is deliberately NOT distinguished: classify never
        // reads bodies outside the Internal fallback, and that 404
        // body's shape is unverified on the wire (only the openapi
        // DECLARES the message) — the honest generic `not_found`
        // stands until a capture proves a distinguishable marker.
        S::CONFLICT if is_designer_prune_url(url) => Err(CoreError::SessionNotPrunable {
            id: designer_prune_id(url),
            endpoint: Some(url.to_string()),
        }),
        // 409 on the EAM FORCE route only (07-06 gap 4, the
        // session_not_prunable precedent's force-route edition): a
        // leftover '(forced)' run occupies the task's slot — the
        // gateway answers 409 with its own Jetty error page ("Task
        // 'X (forced)' already exists! It must be completed or
        // deleted before another task of this type can be force
        // executed."; live-captured 8.3.3, 07-UAT test 7). The
        // page's MESSAGE rides the refusal verbatim; a 409 without
        // the page keeps the '(forced)' fallback detail. Every other
        // route's 409 keeps the Internal fallback below.
        S::CONFLICT if is_eam_force_url(url) => {
            let body = resp.text().await.unwrap_or_default();
            let detail = html_error_parts(&body)
                .map(|(_, message)| message)
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| {
                    "the previous '(forced)' run must be completed or deleted first".to_string()
                });
            Err(CoreError::EamTaskInFlight {
                task: eam_force_task_name(url),
                detail,
                endpoint: Some(url.to_string()),
            })
        }
        // A 422 on a config-RESOURCE path (07-05 gap 3): the gateway
        // rejected a client-composed resource BODY — validation, not
        // an internal error. Path-scoped (the EAM create path
        // `/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks`
        // live-answers 422 `{"messages":["Settings cannot be
        // null"],"fieldMessages":[]}` on 8.3.3) so runtime endpoints
        // keep the Internal fallback. The `messages` array joins into
        // the reason; a non-JSON body rides its raw text; an empty
        // body stays a bare 422 note (the EamNotController arm's
        // body-reading precedent).
        S::UNPROCESSABLE_ENTITY if is_config_resource_url(url) => {
            let body = resp.text().await.unwrap_or_default();
            let joined = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|parsed| {
                    parsed["messages"].as_array().map(|messages| {
                        messages
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                })
                .filter(|joined| !joined.is_empty());
            let reason = match joined {
                Some(joined) => {
                    format!("gateway rejected the resource body (HTTP 422 from {url}): {joined}")
                }
                None if !body.trim().is_empty() => {
                    format!("gateway rejected the resource body (HTTP 422 from {url}): {body}")
                }
                None => format!("gateway rejected the resource body (HTTP 422 from {url})"),
            };
            Err(CoreError::InvalidInput { reason })
        }
        _ => {
            // Unclassifiable: if the body is the Jetty HTML error page,
            // surface its own title/message instead of a bare status.
            let is_html = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|ct| ct.to_ascii_lowercase().contains("text/html"));
            let detail = if is_html {
                let body = resp.text().await.unwrap_or_default();
                html_error_parts(&body).map(|(code, message)| {
                    format!(" (gateway error page: Error {code}: {message})")
                })
            } else {
                None
            };
            Err(CoreError::Internal(format!(
                "unexpected HTTP {status} from {url}{}",
                detail.unwrap_or_default()
            )))
        }
    }
}

/// Is `url` on the EAM runtime surface (`/data/eam/`)? The
/// controller-403 classification is scoped to this prefix — the
/// module-scoped seam (the designer-prune route scoping precedent).
fn is_eam_url(url: &str) -> bool {
    url.contains("/data/eam/")
}

/// Is `url` on the config-RESOURCE surface
/// (`/data/api/v1/resources/`)? The 422 body-rejection arm is
/// scoped to this prefix — resource create/PUT bodies are
/// client-composed, so a 422 is OUR payload failing the server's
/// validation (`invalid_input`); everything else keeps the Internal
/// fallback.
fn is_config_resource_url(url: &str) -> bool {
    url.contains("/data/api/v1/resources/")
}

/// Is `url` on the EAM FORCE route
/// (`…/data/eam/api/v1/eam-tasks/force/…`)? The in-flight 409 arm
/// is scoped to this prefix — the history route and the
/// config-resource definition paths keep the Internal fallback.
fn is_eam_force_url(url: &str) -> bool {
    url.contains("/data/eam/api/v1/eam-tasks/force/")
}

/// The forced task's name — the LAST path segment after the
/// force-route prefix (the URL is `/eam-tasks/force/{owner}/{name}`;
/// query-safe like [`designer_prune_id`]).
fn eam_force_task_name(url: &str) -> String {
    url.split_once("/data/eam/api/v1/eam-tasks/force/")
        .map(|(_, tail)| {
            tail.split('?')
                .next()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

/// Is `url` the SINGULAR designer-prune route
/// (`…/data/api/v1/designer/{id}`)? The trailing `/designer/` segment
/// cannot match the plural `/data/api/v1/designers` list — the `s`
/// closes the path segment before any slash appears.
fn is_designer_prune_url(url: &str) -> bool {
    url.contains("/data/api/v1/designer/")
}

/// The pruned session id — the path segment after the prune-route
/// prefix (query-safe: anything from `?` on is not part of the id; the
/// prune DELETE carries no query params today, this is just honest
/// plumbing).
fn designer_prune_id(url: &str) -> String {
    url.split_once("/data/api/v1/designer/")
        .map(|(_, tail)| {
            tail.split('?')
                .next()
                .unwrap_or_default()
                .trim_end_matches('/')
                .to_string()
        })
        .unwrap_or_default()
}

/// Extract `(status, message)` from the fixed Jetty error-page template
/// via substring scan — `<title>Error NNN</title>` and
/// `<th>MESSAGE:</th><td>…</td>`. Returns `None` when either anchor is
/// absent (never guess: the fallback keeps its bare status text).
fn html_error_parts(body: &str) -> Option<(u16, String)> {
    const TITLE_ANCHOR: &str = "<title>Error ";
    const TITLE_END: &str = "</title>";
    const MESSAGE_ANCHOR: &str = "<th>MESSAGE:</th><td>";
    const MESSAGE_END: &str = "</td>";

    let title_start = body.find(TITLE_ANCHOR)? + TITLE_ANCHOR.len();
    let title_end = title_start + body[title_start..].find(TITLE_END)?;
    let code: u16 = body[title_start..title_end].trim().parse().ok()?;

    let message = body
        .find(MESSAGE_ANCHOR)
        .map(|start| {
            let start = start + MESSAGE_ANCHOR.len();
            let end = body[start..]
                .find(MESSAGE_END)
                .map_or(body.len(), |relative| start + relative);
            body[start..end].to_string()
        })
        .unwrap_or_default();

    Some((code, message))
}

#[cfg(test)]
mod tests {
    use super::{
        designer_prune_id, eam_force_task_name, html_error_parts, is_config_resource_url,
        is_designer_prune_url, is_eam_force_url, is_eam_url,
    };

    /// The 422 arm's path scoping (07-05): the EAM create path (and
    /// every config-resource path) matches; the EAM runtime paths
    /// and gateway-info do NOT (they keep the Internal fallback).
    #[test]
    fn config_resource_url_detection_scopes_the_422_arm() {
        assert!(is_config_resource_url(
            "http://gw:8088/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks"
        ));
        assert!(is_config_resource_url(
            "http://gw:8088/data/api/v1/resources/list/com.inductiveautomation.eam/eam-tasks"
        ));
        assert!(!is_config_resource_url(
            "http://gw:8088/data/eam/api/v1/eam-tasks/history"
        ));
        assert!(!is_config_resource_url(
            "http://gw:8088/data/api/v1/gateway-info"
        ));
    }

    /// The EAM controller-403 scoping (07-02): the runtime prefix
    /// matches; the config-resource definition paths do NOT (they
    /// answer normally on stock gateways — definitions are plain
    /// config resources).
    #[test]
    fn eam_url_detection_is_the_runtime_prefix() {
        assert!(is_eam_url(
            "http://gw:8088/data/eam/api/v1/eam-tasks/history"
        ));
        assert!(is_eam_url(
            "http://gw:8088/data/eam/api/v1/eam-tasks/force/eam/t1"
        ));
        assert!(!is_eam_url(
            "http://gw:8088/data/api/v1/resources/list/com.inductiveautomation.eam/eam-tasks"
        ));
        assert!(!is_eam_url("http://gw:8088/data/api/v1/gateway-info"));
    }

    /// The EXACT Jetty error page captured from the live 8.3.6 gateway
    /// (02-RESEARCH §Code Examples) — the golden fixture for the sniffer.
    const CAPTURED_401_HTML: &str = r#"<html><head><meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/><title>Error 401</title></head><body><h2>HTTP ERROR 401 Unauthorized</h2><table><tr><th>URI:</th><td>/data/api/v1/gateway-info</td></tr><tr><th>STATUS:</th><td>401</td></tr><tr><th>MESSAGE:</th><td>Unauthorized</td></tr></table></body></html>"#;

    /// The raw 401 page re-captured verbatim from the still-running
    /// research rig (curl, 2026-08-21): the same fixed template WITH its
    /// inter-row newlines and blank line before `</body>` — pins that the
    /// substring anchors tolerate the wire formatting, not just the
    /// compacted doc form.
    const CAPTURED_401_HTML_RAW: &str = "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 401</title>\n</head>\n<body><h2>HTTP ERROR 401 Unauthorized</h2>\n<table>\n<tr><th>URI:</th><td>/data/api/v1/gateway-info</td></tr>\n<tr><th>STATUS:</th><td>401</td></tr>\n<tr><th>MESSAGE:</th><td>Unauthorized</td></tr>\n</table>\n\n</body>\n</html>\n";

    /// Same fixed template with a 500 title/message — proves the scan is
    /// template-driven, not a 401 hardcode.
    const CAPTURED_500_HTML: &str = r#"<html><head><meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/><title>Error 500</title></head><body><h2>HTTP ERROR 500 Server Error</h2><table><tr><th>URI:</th><td>/data/api/v1/gateway-info</td></tr><tr><th>STATUS:</th><td>500</td></tr><tr><th>MESSAGE:</th><td>Server Error</td></tr></table></body></html>"#;

    #[test]
    fn sniffs_the_captured_jetty_401_page() {
        assert_eq!(
            html_error_parts(CAPTURED_401_HTML),
            Some((401, "Unauthorized".to_string()))
        );
    }

    #[test]
    fn sniffs_the_raw_wire_capture_with_newlines() {
        assert_eq!(
            html_error_parts(CAPTURED_401_HTML_RAW),
            Some((401, "Unauthorized".to_string()))
        );
    }

    #[test]
    fn sniffs_the_500_template_too() {
        assert_eq!(
            html_error_parts(CAPTURED_500_HTML),
            Some((500, "Server Error".to_string()))
        );
    }

    #[test]
    fn returns_none_for_non_template_bodies() {
        assert_eq!(html_error_parts("<html><body>welcome</body></html>"), None);
        assert_eq!(html_error_parts(""), None);
        assert_eq!(html_error_parts("{\"message\":\"json\"}"), None);
    }

    /// Route-scoping of the 409 arm (06-07): the SINGULAR prune path
    /// matches — the plural designers LIST path does not (its `s`
    /// closes the segment), and neither does any other route.
    #[test]
    fn designer_prune_route_detection_is_exact() {
        assert!(is_designer_prune_url(
            "http://gw:8088/data/api/v1/designer/d-live-1"
        ));
        assert!(
            !is_designer_prune_url("http://gw:8088/data/api/v1/designers"),
            "the plural list route must NOT match"
        );
        assert!(
            !is_designer_prune_url("http://gw:8088/data/api/v1/designers?limit=1"),
            "the list route with query params must NOT match"
        );
        assert!(!is_designer_prune_url(
            "http://gw:8088/data/perspective/api/v1/sessions"
        ));
    }

    /// The pruned id rides the trailing path segment (query-safe).
    #[test]
    fn designer_prune_id_extracts_the_trailing_segment() {
        assert_eq!(
            designer_prune_id("http://gw:8088/data/api/v1/designer/d-live-1"),
            "d-live-1"
        );
        assert_eq!(
            designer_prune_id("http://gw:8088/data/api/v1/designer/10443A91?x=1"),
            "10443A91"
        );
    }

    /// The force-409 arm's path scoping (07-06 gap 4): the FORCE
    /// prefix matches; the history route and the config-resource
    /// definition path do NOT (they keep the Internal fallback).
    #[test]
    fn eam_force_url_detection_scopes_the_409_arm() {
        assert!(is_eam_force_url(
            "http://gw:8088/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup"
        ));
        assert!(!is_eam_force_url(
            "http://gw:8088/data/eam/api/v1/eam-tasks/history"
        ));
        assert!(!is_eam_force_url(
            "http://gw:8088/data/api/v1/resources/list/com.inductiveautomation.eam/eam-tasks"
        ));
    }

    /// The forced task's name is the LAST force-route segment after
    /// the owner (query-safe like `designer_prune_id`).
    #[test]
    fn eam_force_task_name_extracts_the_trailing_segment() {
        assert_eq!(
            eam_force_task_name(
                "http://gw:8088/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup"
            ),
            "cli-research-backup"
        );
        assert_eq!(
            eam_force_task_name(
                "http://gw:8088/data/eam/api/v1/eam-tasks/force/eam/nightly-backup?x=1"
            ),
            "nightly-backup"
        );
    }
}
