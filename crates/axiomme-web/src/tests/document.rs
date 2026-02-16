use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::util::ServiceExt;

use axiomme_core::models::MarkdownSaveResult;

use super::harness::{TestHarness, decode_json, json_request};

#[tokio::test]
async fn web_document_load_readonly_is_locked_during_in_flight_save() {
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
                .uri("/api/document?uri=axiom://resources/web-editor/events.jsonl")
                .body(Body::empty())
                .expect("document request"),
        )
        .await
        .expect("document response");
    assert_eq!(blocked.status(), StatusCode::LOCKED);
    let payload: serde_json::Value = decode_json(blocked).await;
    assert_eq!(payload["code"], "LOCKED");
}

#[tokio::test]
async fn web_document_load_markdown_is_editable() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/document?uri={}", harness.uri))
                .body(Body::empty())
                .expect("document request"),
        )
        .await
        .expect("document response");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["format"], "markdown");
    assert_eq!(payload["editable"], true);
    assert!(payload["etag"].as_str().is_some_and(|x| !x.is_empty()));
}

#[tokio::test]
async fn web_document_load_json_is_editable_and_pretty() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/document?uri=axiom://resources/web-editor/config.json")
                .body(Body::empty())
                .expect("document request"),
        )
        .await
        .expect("document response");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["format"], "json");
    assert_eq!(payload["editable"], true);
    let content = payload["content"].as_str().expect("content str");
    assert!(content.contains("\"name\""));
    assert!(content.contains("axiomme"));
    assert!(payload["etag"].as_str().is_some_and(|x| !x.is_empty()));
}

#[tokio::test]
async fn web_document_save_json_success() {
    let harness = TestHarness::setup();

    let load = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/document?uri=axiom://resources/web-editor/config.json")
                .body(Body::empty())
                .expect("load request"),
        )
        .await
        .expect("load response");
    assert_eq!(load.status(), StatusCode::OK);
    let loaded: serde_json::Value = decode_json(load).await;
    let etag = loaded["etag"].as_str().expect("etag").to_string();

    let save = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/document/save",
            json!({
                "uri": "axiom://resources/web-editor/config.json",
                "content": "{\n  \"name\": \"axiomme\",\n  \"v\": 2\n}",
                "expected_etag": etag
            }),
        ))
        .await
        .expect("save response");
    assert_eq!(save.status(), StatusCode::OK);
    let saved: MarkdownSaveResult = decode_json(save).await;
    assert!(!saved.etag.is_empty());

    let after = harness
        .state
        .app
        .read("axiom://resources/web-editor/config.json")
        .expect("read after save");
    assert!(after.contains("\"v\": 2"));
}

#[tokio::test]
async fn web_document_save_json_invalid_returns_400() {
    let harness = TestHarness::setup();

    let load = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/document?uri=axiom://resources/web-editor/config.json")
                .body(Body::empty())
                .expect("load request"),
        )
        .await
        .expect("load response");
    assert_eq!(load.status(), StatusCode::OK);
    let loaded: serde_json::Value = decode_json(load).await;
    let etag = loaded["etag"].as_str().expect("etag").to_string();

    let save = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/document/save",
            json!({
                "uri": "axiom://resources/web-editor/config.json",
                "content": "{\"name\":",
                "expected_etag": etag
            }),
        ))
        .await
        .expect("save response");
    assert_eq!(save.status(), StatusCode::BAD_REQUEST);
    let payload: serde_json::Value = decode_json(save).await;
    assert_eq!(payload["code"], "VALIDATION_FAILED");

    let after = harness
        .state
        .app
        .read("axiom://resources/web-editor/config.json")
        .expect("read after failed save");
    assert_eq!(after, "{\"name\":\"axiomme\",\"v\":1}");
}

#[tokio::test]
async fn web_document_load_rejects_unsupported_format() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/document?uri=axiom://resources/web-editor/script.sh")
                .body(Body::empty())
                .expect("document request"),
        )
        .await
        .expect("document response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["code"], "VALIDATION_FAILED");
}

#[tokio::test]
async fn web_document_load_generated_tier_markdown_is_readonly() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/document?uri=axiom://resources/web-editor/.overview.md")
                .body(Body::empty())
                .expect("document request"),
        )
        .await
        .expect("document response");
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["format"], "markdown");
    assert_eq!(payload["editable"], false);
    assert!(payload["content"].as_str().is_some_and(|x| !x.is_empty()));
}
