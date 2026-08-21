//! IgnitionMock — the shared wiremock harness for ignition-core
//! integration tests (02-01): thin builders that speak the two fixture
//! shapes a real 8.3 gateway produces (02-RESEARCH §Code Examples) —
//! the `{items, metadata}` list envelope and the fixed Jetty HTML error
//! page — so later capability plans' tests stay ~3 lines per scenario.
//!
//! Usage: `let mock = IgnitionMock::start().await;` then mount one of
//! the builders (each registers with `expect(1)` — verified when the
//! server drops) and point `ReqwestGatewayApi::for_tests(&mock.uri(), …)`
//! at it.
//!
//! `allow(dead_code)`: this is a harness library — individual builders
//! go unused until the plan that needs them lands.

#![allow(dead_code)]

/// Wraps a [`wiremock::MockServer`] with Ignition-shaped fixture builders.
pub struct IgnitionMock {
    /// The underlying server — mount bespoke mocks directly when a test
    /// needs something the builders don't cover.
    pub server: wiremock::MockServer,
}

impl IgnitionMock {
    /// Start a fresh mock gateway.
    pub async fn start() -> Self {
        Self {
            server: wiremock::MockServer::start().await,
        }
    }

    /// Base URI to point a client at.
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// 200 + JSON body — the standard list-envelope shape
    /// (`{items: […], metadata: {…}}`).
    pub async fn list_json(&self, method: &str, path: &str, body: serde_json::Value) {
        wiremock::Mock::given(wiremock::matchers::method(method))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&self.server)
            .await
    }

    /// `status` + the EXACT Jetty HTML error page the gateway returns for
    /// `/data/api/v1/*` failures (title `Error NNN`, MESSAGE row) — the
    /// body shape that crashes naive `.json()` calls (Pitfall 3).
    pub async fn html_error(&self, method: &str, path: &str, status: u16) {
        wiremock::Mock::given(wiremock::matchers::method(method))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(status).set_body_raw(
                // set_body_raw (not set_body_string + insert_header):
                // wiremock's mime field always overwrites inserted
                // Content-Type headers, and the gateway answers with the
                // exact `text/html;charset=iso-8859-1` the classifier
                // sniffs on.
                jetty_error_html(status, path),
                "text/html;charset=iso-8859-1",
            ))
            .expect(1)
            .mount(&self.server)
            .await
    }

    /// 302 + `Location` header — an uncommissioned gateway redirects
    /// everything to `/welcome`; other targets (e.g. `/idp`) exist too.
    pub async fn redirect(&self, path: &str, location: &str) {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(302).insert_header("Location", location))
            .expect(1)
            .mount(&self.server)
            .await
    }

    /// Generic `status` + JSON body (e.g. 404
    /// `{"message": "No route match for path: …"}`).
    pub async fn status_json(
        &self,
        method: &str,
        path: &str,
        status: u16,
        body: serde_json::Value,
    ) {
        wiremock::Mock::given(wiremock::matchers::method(method))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(body))
            .expect(1)
            .mount(&self.server)
            .await
    }

    /// 200 with the literal body `true` — the verified restart-POST
    /// response shape (ready for 02-05).
    pub async fn literal_true(&self, method: &str, path: &str) {
        wiremock::Mock::given(wiremock::matchers::method(method))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
            .expect(1)
            .mount(&self.server)
            .await
    }
}

/// The fixed Jetty error-page template, byte-shaped after the page
/// captured from the live 8.3.6 gateway (02-RESEARCH §Code Examples),
/// parameterized by status/URI. The classifier's sniffer keys on
/// `<title>Error NNN</title>` and `<th>MESSAGE:</th><td>…</td>`.
pub fn jetty_error_html(status: u16, uri: &str) -> String {
    let message = match status {
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    };
    format!(
        concat!(
            r#"<html><head><meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>"#,
            r#"<title>Error {status}</title></head><body><h2>HTTP ERROR {status} {message}</h2><table>"#,
            r#"<tr><th>URI:</th><td>{uri}</td></tr><tr><th>STATUS:</th><td>{status}</td></tr>"#,
            r#"<tr><th>MESSAGE:</th><td>{message}</td></tr></table></body></html>"#
        ),
        status = status,
        message = message,
        uri = uri,
    )
}
