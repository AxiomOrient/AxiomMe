use axum::{
    body::{Body, to_bytes},
    http::{
        Request, StatusCode,
        header::{self, CONTENT_TYPE},
    },
};
use tower::util::ServiceExt;

use super::harness::{TestHarness, header_value};

#[tokio::test]
async fn web_responses_include_security_headers() {
    let harness = TestHarness::setup();

    let index = get(&harness, "/").await;
    assert_eq!(index.status(), StatusCode::OK);
    let index_body = to_bytes(index.into_body(), usize::MAX)
        .await
        .expect("read index body");
    let index_html = String::from_utf8(index_body.to_vec()).expect("index utf8");
    assert!(index_html.contains("Filesystem"));
    assert!(index_html.contains("Load Tree"));
    assert!(index_html.contains("/assets/index.css"));
    assert!(index_html.contains("/assets/index.js"));

    let index_for_headers = get(&harness, "/").await;
    assert_eq!(
        header_value(index_for_headers.headers(), "x-content-type-options"),
        Some("nosniff")
    );
    assert_eq!(
        header_value(index_for_headers.headers(), "x-frame-options"),
        Some("DENY")
    );
    assert_eq!(
        header_value(index_for_headers.headers(), "referrer-policy"),
        Some("no-referrer")
    );
    let csp =
        header_value(index_for_headers.headers(), "content-security-policy").expect("csp header");
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(!csp.contains("'unsafe-inline'"));

    let css_response = get(&harness, "/assets/index.css").await;
    assert_eq!(css_response.status(), StatusCode::OK);
    assert_eq!(
        css_response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/css; charset=utf-8")
    );

    let js = get(&harness, "/assets/index.js").await;
    assert_eq!(js.status(), StatusCode::OK);
    assert_eq!(
        js.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/javascript; charset=utf-8")
    );

    let api_path = format!("/api/markdown?uri={}", harness.uri);
    let api = get(&harness, &api_path).await;
    assert_eq!(api.status(), StatusCode::OK);
    assert_eq!(
        header_value(api.headers(), "x-content-type-options"),
        Some("nosniff")
    );
    assert!(header_value(api.headers(), "content-security-policy").is_some());
}

async fn get(harness: &TestHarness, uri: &str) -> axum::http::Response<Body> {
    harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
}
