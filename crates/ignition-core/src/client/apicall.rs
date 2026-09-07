//! The raw api-call capability (09-03, EXT-01) — `ign api call`'s
//! client half: the request model, the gateway-verbatim data
//! envelope, and the two usage-class guards the CLI runs
//! PRE-resolution (and the action re-runs).
//!
//! THE verbatim decision (research OQ1, RawValue): the success body
//! is captured as TEXT and embedded via
//! [`serde_json::value::RawValue::from_string`] — one call that both
//! preserves the gateway's bytes (key order, unknown fields, number
//! shapes) AND validates JSON. "Gateway-verbatim" is pinned to mean:
//! no field dropped, no value coerced, key order preserved. A 2xx
//! body that is NOT JSON refuses [`CoreError::Internal`] with an
//! explanatory message — binary endpoints ride the logs/backup
//! download pipelines, never this one (README's documented contract
//! exception).
//!
//! THE refusal rule (research Pattern 2): user-supplied auth-pattern
//! headers (`authorization`, `x-ignition-api-token`, `cookie` —
//! case-insensitive, trimmed: HTTP header names are case-insensitive
//! and trimming beats whitespace-prefix smuggling) are REFUSED, never
//! stripped or silently overridden. `apply_auth` is the ONE
//! auth-header site (the redaction boundary, CORE-02); a
//! caller-supplied auth header would either shadow the profile
//! credential or double-send — both dishonest, so the guard refuses
//! loudly BEFORE any I/O.
//!
//! THE path rules (research OQ4/Pitfall 4 — one mechanism each): a
//! leading `/` is required (`url_for` joins onto the profile URL);
//! an absolute/foreign-host URL is refused (the join would silently
//! rebase the request off the profile's gateway); `?` in the path is
//! refused in favor of the ONE query mechanism, repeatable
//! `--query k=v`.

use crate::error::CoreError;

/// Headers the raw passthrough REFUSES — auth comes from the profile,
/// full stop (EXT-01 success criterion 4). Compared case-insensitively
/// against the TRIMMED header name (Pitfall 3).
const REFUSED_AUTH_HEADERS: [&str; 3] = ["authorization", "x-ignition-api-token", "cookie"];

/// The raw passthrough request: arbitrary method, caller path, raw
/// body text, extra headers, and query pairs. `body` is RAW TEXT
/// passthrough (JSON or otherwise — the gateway's answer classifies;
/// GET/DELETE bodies allowed, curl parity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCallRequest {
    /// HTTP method (any RFC verb — reqwest validates the bytes).
    pub method: String,
    /// Absolute path on the gateway (leading `/`, no `?`, no host).
    pub path: String,
    /// Raw body text, ANY method (None = no body).
    pub body: Option<String>,
    /// Extra headers (auth-pattern names are refused pre-I/O).
    pub headers: Vec<(String, String)>,
    /// Query pairs (the ONE query mechanism — never `?` in `path`).
    pub query: Vec<(String, String)>,
}

/// The gateway's answer: HTTP status plus the body VERBATIM
/// ([`serde_json::value::RawValue`] — bytes preserved, key order
/// kept, still valid JSON; serializes inline inside the envelope).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiCallData {
    /// The HTTP status the gateway answered with.
    pub status: u16,
    /// The response body VERBATIM.
    pub data: Box<serde_json::value::RawValue>,
}

/// Refuse user-supplied auth-pattern headers — pre-I/O, usage class
/// (exit 2). The comparison is `trim().to_ascii_lowercase()` on the
/// NAME only: HTTP header names are case-insensitive (Pitfall 3), and
/// trimming beats whitespace-prefix smuggling. The refusal names the
/// header and the profile-auth rule so the fix is obvious.
pub fn refuse_auth_headers(headers: &[(String, String)]) -> Result<(), CoreError> {
    for (name, _) in headers {
        let normalized = name.trim().to_ascii_lowercase();
        if REFUSED_AUTH_HEADERS.contains(&normalized.as_str()) {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "header {name:?} is auth-pattern and refused — credentials come \
                     from the profile (ign applies X-Ignition-API-Token itself); \
                     pass only non-auth headers"
                ),
            });
        }
    }
    Ok(())
}

/// Validate `--path` — pre-I/O, usage class (exit 2):
///
/// (a) must start with `/` (it joins onto the profile URL);
/// (b) must not be host-shaped — a protocol-relative `//host/…`
///     prefix or any string [`url::Url`] parses WITH a host
///     (absolute/foreign URLs) is refused: `url_for`'s join would
///     silently rebase the request off the profile's gateway, turning
///     the CLI into a proxy;
/// (c) must not embed `?` — the ONE query mechanism is the repeatable
///     `--query k=v` flag (Pitfall 4: double-encoded query params are
///     a silent wire corruption).
pub fn validate_path(path: &str) -> Result<(), CoreError> {
    // Host-shaped FIRST — the most specific diagnosis: an absolute or
    // protocol-relative URL would make `url_for`'s join silently
    // rebase the request off the profile's gateway (the CLI must not
    // become a proxy), and that cause deserves its own refusal text
    // even when the leading-slash rule would also have fired.
    if path.starts_with("//")
        || url::Url::parse(path).is_ok_and(|parsed| parsed.host_str().is_some())
    {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "--path must be a path, not a URL ({path:?} carries a host) — the \
                 request always goes to the profile's gateway"
            ),
        });
    }
    if !path.starts_with('/') {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "--path must start with '/' (it joins onto the profile's gateway URL): {path:?}"
            ),
        });
    }
    if path.contains('?') {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "--path must not embed a query string ({path:?}) — use the repeatable \
                 --query k=v flag (the ONE query mechanism)"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ApiCallRequest, REFUSED_AUTH_HEADERS, refuse_auth_headers, validate_path};
    use crate::client::GatewayApi;
    use crate::client::ReqwestGatewayApi;
    use crate::config::{Credential, Secret};
    use crate::error::CoreError;

    // ---- refusal matrix (free function, Pitfall 3's full set) ----

    #[test]
    fn refusal_matrix_covers_case_and_whitespace_variants() {
        let canonical = [("Authorization".to_string(), "Bearer x".to_string())];
        let case_variant = [("AUTHORIZATION".to_string(), "Bearer x".to_string())];
        let mixed_case = [("authorization".to_string(), "Bearer x".to_string())];
        let token = [("x-ignition-api-token".to_string(), "name:key".to_string())];
        let token_canonical = [("X-Ignition-API-Token".to_string(), "name:key".to_string())];
        let cookie = [("Cookie".to_string(), "session=1".to_string())];
        let whitespace = [("  Authorization ".to_string(), "Bearer x".to_string())];

        for headers in [
            &canonical,
            &case_variant,
            &mixed_case,
            &token,
            &token_canonical,
            &cookie,
            &whitespace,
        ] {
            let err = refuse_auth_headers(headers).expect_err("auth-pattern header refuses");
            assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
            assert_eq!(err.code(), "invalid_input");
            assert_eq!(err.exit_code(), 2, "usage class");
            let text = err.to_string();
            assert!(
                text.contains("profile"),
                "the refusal names the profile-auth rule: {text}"
            );
        }

        // Non-auth headers pass.
        refuse_auth_headers(&[("X-Custom-Thing".to_string(), "v".to_string())])
            .expect("non-auth header passes");
        refuse_auth_headers(&[]).expect("no headers passes");
    }

    /// The refused set is exactly the three documented names (a fourth
    /// addition is a contract change — pin it loudly).
    #[test]
    fn refused_set_is_exactly_the_documented_three() {
        assert_eq!(REFUSED_AUTH_HEADERS.len(), 3);
        assert!(REFUSED_AUTH_HEADERS.contains(&"authorization"));
        assert!(REFUSED_AUTH_HEADERS.contains(&"x-ignition-api-token"));
        assert!(REFUSED_AUTH_HEADERS.contains(&"cookie"));
    }

    // ---- path matrix (free function, Pitfall 4's full set) ----

    #[test]
    fn path_matrix_refuses_bad_shapes_and_accepts_clean_paths() {
        // Refusals, each with a specific reason.
        let missing_slash = validate_path("data/api/v1/x").expect_err("missing slash refuses");
        assert!(missing_slash.to_string().contains("'/'"), "{missing_slash}");

        let absolute = validate_path("http://other/x").expect_err("absolute URL refuses");
        assert!(absolute.to_string().contains("host"), "{absolute}");

        let protocol_relative = validate_path("//host/x").expect_err("//host refuses");
        assert!(
            protocol_relative.to_string().contains("host"),
            "{protocol_relative}"
        );

        let query_embedded = validate_path("/data/x?embed=1").expect_err("embedded ? refuses");
        assert!(
            query_embedded.to_string().contains("--query"),
            "the ? refusal names the ONE query mechanism: {query_embedded}"
        );

        // Clean paths pass.
        validate_path("/data/x").expect("clean single-segment path passes");
        validate_path("/data/api/v1/gateway-info").expect("multi-segment path passes");
        validate_path("/").expect("root path passes");
    }

    // ---- the pipeline over wiremock ----

    fn token_client(base: &str) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(base, Some(Credential::Token(Secret::new("name:key"))))
    }

    /// THE verbatim path: a 2xx body rides through `RawValue` — the
    /// odd key order survives (the binary contract test pins the same
    /// property end-to-end; this is the unit-level proof).
    #[tokio::test]
    async fn success_body_rides_verbatim_with_status() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/x"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"zz_last": 1, "alpha_first": {"b": 2, "a": 1}}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let call = ApiCallRequest {
            method: "GET".to_string(),
            path: "/data/api/v1/x".to_string(),
            body: None,
            headers: vec![],
            query: vec![],
        };
        let data = token_client(&server.uri())
            .api_call(&call)
            .await
            .expect("2xx answers");
        assert_eq!(data.status, 200);
        assert_eq!(
            data.data.get(),
            r#"{"zz_last": 1, "alpha_first": {"b": 2, "a": 1}}"#
        );
    }

    /// The request REACHES the gateway with the profile's auth header,
    /// the query pairs appended, and the body riding a GET (curl
    /// parity — decision 6).
    #[tokio::test]
    async fn request_rides_auth_query_and_get_body() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/x"))
            .and(wiremock::matchers::query_param("a", "1"))
            .and(wiremock::matchers::query_param("b", "2"))
            .and(wiremock::matchers::header(
                "x-ignition-api-token",
                "name:key",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&server)
            .await;

        let call = ApiCallRequest {
            method: "GET".to_string(),
            path: "/data/api/v1/x".to_string(),
            body: Some(r#"{"x":1}"#.to_string()),
            headers: vec![("X-Custom".to_string(), "v".to_string())],
            query: vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
            ],
        };
        token_client(&server.uri())
            .api_call(&call)
            .await
            .expect("mock answers");

        let requests = server.received_requests().await.expect("requests recorded");
        assert_eq!(requests.len(), 1);
        let body = String::from_utf8_lossy(&requests[0].body);
        assert_eq!(body, r#"{"x":1}"#, "the GET carries the raw body verbatim");
        let custom = requests[0]
            .headers
            .iter()
            .find(|(name, _)| name.as_str() == "x-custom")
            .map(|(_, value)| value.to_str().expect("ascii").to_string());
        assert_eq!(custom.as_deref(), Some("v"), "the user header rode along");
    }

    /// A non-JSON 2xx body is the honest internal-class refusal with
    /// the explanatory message (the README documents it — the binary
    /// endpoints belong to the download pipelines).
    #[tokio::test]
    async fn non_json_2xx_body_refuses_internal() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/x"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("this is not json"))
            .expect(1)
            .mount(&server)
            .await;

        let call = ApiCallRequest {
            method: "GET".to_string(),
            path: "/data/api/v1/x".to_string(),
            body: None,
            headers: vec![],
            query: vec![],
        };
        let err = token_client(&server.uri())
            .api_call(&call)
            .await
            .expect_err("non-JSON 2xx refuses");
        assert!(matches!(err, CoreError::Internal(_)), "{err}");
        assert_eq!(err.exit_code(), 1);
        let text = err.to_string();
        assert!(
            text.contains("non-JSON body"),
            "the message explains the contract: {text}"
        );
    }

    /// A syntactically INVALID verb string refuses pre-I/O (usage
    /// class), never a reqwest panic. (Arbitrary well-formed
    /// extension methods stay accepted — reqwest's `Method` parser is
    /// the validator; the gateway's answer classifies.)
    #[tokio::test]
    async fn invalid_method_refuses_invalid_input() {
        let server = wiremock::MockServer::start().await;
        for method in ["NOT A VERB", "GE\tT", ""] {
            let call = ApiCallRequest {
                method: method.to_string(),
                path: "/data/x".to_string(),
                body: None,
                headers: vec![],
                query: vec![],
            };
            let err = token_client(&server.uri())
                .api_call(&call)
                .await
                .expect_err("invalid verb refuses");
            assert!(
                matches!(err, CoreError::InvalidInput { .. }),
                "{method:?} refuses invalid_input: {err}"
            );
        }
        // Zero requests: the refusal fired before the wire.
        let requests = server.received_requests().await.expect("requests recorded");
        assert!(
            requests.is_empty(),
            "no request may hit the wire: {requests:?}"
        );
    }
}
