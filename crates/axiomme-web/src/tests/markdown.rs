use std::fs;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::util::ServiceExt;

use axiomme_core::AxiomUri;
use axiomme_core::models::{MarkdownDocument, MarkdownSaveResult};

use super::harness::{TestHarness, decode_json, json_request};

#[tokio::test]
async fn web_markdown_s1_load_save_success() {
    let harness = TestHarness::setup();

    let load = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/markdown?uri={}", harness.uri))
                .body(Body::empty())
                .expect("load request"),
        )
        .await
        .expect("load response");
    assert_eq!(load.status(), StatusCode::OK);
    let loaded: MarkdownDocument = decode_json(load).await;

    let save = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/save",
            json!({
                "uri": harness.uri,
                "content": "# Guide\n\nbeta_token",
                "expected_etag": loaded.etag
            }),
        ))
        .await
        .expect("save response");
    assert_eq!(save.status(), StatusCode::OK);
    let saved: MarkdownSaveResult = decode_json(save).await;
    assert_eq!(saved.reindexed_root, harness.parent_uri);
    assert!(!saved.etag.is_empty());

    let find = harness
        .state
        .app
        .find(
            "beta_token",
            Some(&harness.parent_uri),
            Some(10),
            None,
            None,
        )
        .expect("find after save");
    assert!(
        find.resources.iter().any(|hit| hit.uri == harness.uri)
            || find.memories.iter().any(|hit| hit.uri == harness.uri)
            || find.skills.iter().any(|hit| hit.uri == harness.uri),
        "saved markdown should be searchable immediately"
    );
}

#[tokio::test]
async fn web_markdown_s2_stale_etag_returns_conflict() {
    let harness = TestHarness::setup();

    let load = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/markdown?uri={}", harness.uri))
                .body(Body::empty())
                .expect("load request"),
        )
        .await
        .expect("load response");
    assert_eq!(load.status(), StatusCode::OK);
    let loaded: MarkdownDocument = decode_json(load).await;

    let first_save = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/save",
            json!({
                "uri": harness.uri,
                "content": "# Guide\n\netag_v2",
                "expected_etag": loaded.etag
            }),
        ))
        .await
        .expect("first save response");
    assert_eq!(first_save.status(), StatusCode::OK);

    let stale_save = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/save",
            json!({
                "uri": harness.uri,
                "content": "# Guide\n\netag_v3",
                "expected_etag": loaded.etag
            }),
        ))
        .await
        .expect("stale save response");
    assert_eq!(stale_save.status(), StatusCode::CONFLICT);
    let payload: serde_json::Value = decode_json(stale_save).await;
    assert_eq!(payload["code"], "CONFLICT");
}

#[tokio::test]
async fn web_markdown_s3_in_flight_lock_returns_423() {
    let harness = TestHarness::setup();
    let _writer_guard = harness
        .state
        .editor_gate
        .try_write()
        .expect("acquire write gate");

    let blocked = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/markdown?uri={}", harness.uri))
                .body(Body::empty())
                .expect("load request"),
        )
        .await
        .expect("load response");
    assert_eq!(blocked.status(), StatusCode::LOCKED);
    let payload: serde_json::Value = decode_json(blocked).await;
    assert_eq!(payload["code"], "LOCKED");
}

#[tokio::test]
async fn web_markdown_save_rejects_queue_scope_with_403() {
    let harness = TestHarness::setup();
    let queue_uri = AxiomUri::parse("axiom://queue/editor/test.md").expect("queue uri");
    harness
        .state
        .app
        .fs
        .write(&queue_uri, "# queue", true)
        .expect("write queue");

    let response = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/save",
            json!({
                "uri": "axiom://queue/editor/test.md",
                "content": "# queue update",
                "expected_etag": serde_json::Value::Null
            }),
        ))
        .await
        .expect("save response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn web_markdown_save_rejects_tier_file_with_403() {
    let harness = TestHarness::setup();
    let tier_uri = AxiomUri::parse("axiom://resources/web-editor/.overview.md").expect("tier");
    harness
        .state
        .app
        .fs
        .write(&tier_uri, "# tier", true)
        .expect("write tier");

    let response = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/save",
            json!({
                "uri": "axiom://resources/web-editor/.overview.md",
                "content": "# update tier",
                "expected_etag": serde_json::Value::Null
            }),
        ))
        .await
        .expect("save response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn web_preview_sanitizes_unsafe_html_and_links() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/preview",
            json!({
                "content": "[bad](javascript:alert(1)) [ok](/docs/readme.md) [proto](//evil.example/path)\n\n<script>alert(1)</script>\n\n![img](data:text/html;base64,abcd)"
            }),
        ))
        .await
        .expect("preview response");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = decode_json(response).await;
    let html = payload["html"].as_str().expect("html str");

    assert!(!html.contains("javascript:"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("data:text/html"));
    assert!(!html.contains("//evil.example/path"));
    assert!(html.contains("href=\"#\""));
    assert!(html.contains("href=\"/docs/readme.md\""));
}

#[cfg(unix)]
#[tokio::test]
async fn web_markdown_save_reindex_failure_returns_500_with_rollback_details() {
    let harness = TestHarness::setup();
    let root_uri = AxiomUri::parse("axiom://resources/web-editor").expect("root parse");
    let bad_path = harness
        .state
        .app
        .fs
        .resolve_uri(&root_uri)
        .join("bad\\name.md");
    fs::write(bad_path, "force reindex failure").expect("write bad path");

    let load = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/markdown?uri={}", harness.uri))
                .body(Body::empty())
                .expect("load request"),
        )
        .await
        .expect("load response");
    assert_eq!(load.status(), StatusCode::OK);
    let loaded: MarkdownDocument = decode_json(load).await;

    let response = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/markdown/save",
            json!({
                "uri": harness.uri,
                "content": "# Guide\n\nmust_fail",
                "expected_etag": loaded.etag
            }),
        ))
        .await
        .expect("save response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["code"], "INTERNAL_ERROR");
    assert!(payload["details"]["rollback_write"].is_string());
    assert!(payload["details"]["rollback_reindex"].is_string());
}
