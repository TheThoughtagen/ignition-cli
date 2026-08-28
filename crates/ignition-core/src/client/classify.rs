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
    use super::{designer_prune_id, html_error_parts, is_designer_prune_url};

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
}
