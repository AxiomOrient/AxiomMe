use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::util::ServiceExt;

use axiomme_core::AxiomUri;

use super::harness::{TestHarness, decode_json, json_request};

#[tokio::test]
async fn web_fs_list_and_tree_success() {
    let harness = TestHarness::setup();

    let list = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/fs/list?uri=axiom://resources/web-editor&recursive=false")
                .body(axum::body::Body::empty())
                .expect("list request"),
        )
        .await
        .expect("list response");
    assert_eq!(list.status(), StatusCode::OK);
    let list_payload: serde_json::Value = decode_json(list).await;
    assert_eq!(list_payload["uri"], "axiom://resources/web-editor");
    assert!(
        list_payload["entries"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry["uri"] == "axiom://resources/web-editor/guide.md" && entry["is_dir"] == false
            }))
    );

    let tree = harness
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/fs/tree?uri=axiom://resources/web-editor")
                .body(axum::body::Body::empty())
                .expect("tree request"),
        )
        .await
        .expect("tree response");
    assert_eq!(tree.status(), StatusCode::OK);
    let tree_payload: serde_json::Value = decode_json(tree).await;
    assert_eq!(tree_payload["root"]["uri"], "axiom://resources/web-editor");
    assert!(
        tree_payload["root"]["children"]
            .as_array()
            .is_some_and(|children| {
                children
                    .iter()
                    .any(|child| child["uri"] == "axiom://resources/web-editor/guide.md")
            })
    );
}

#[tokio::test]
async fn web_fs_mkdir_move_delete_lifecycle() {
    let harness = TestHarness::setup();
    let new_dir = "axiom://resources/web-editor/newdir";
    let moved_dir = "axiom://resources/web-editor/newdir-renamed";

    let mkdir = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/fs/mkdir",
            json!({
                "uri": new_dir
            }),
        ))
        .await
        .expect("mkdir response");
    assert_eq!(mkdir.status(), StatusCode::OK);
    assert!(
        harness
            .state
            .app
            .fs
            .exists(&AxiomUri::parse(new_dir).expect("new_dir parse"))
    );

    let mv = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/fs/move",
            json!({
                "from_uri": new_dir,
                "to_uri": moved_dir
            }),
        ))
        .await
        .expect("move response");
    assert_eq!(mv.status(), StatusCode::OK);
    assert!(
        harness
            .state
            .app
            .fs
            .exists(&AxiomUri::parse(moved_dir).expect("moved_dir parse"))
    );
    assert!(
        !harness
            .state
            .app
            .fs
            .exists(&AxiomUri::parse(new_dir).expect("new_dir parse"))
    );

    let delete = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/fs/delete",
            json!({
                "uri": moved_dir,
                "recursive": true
            }),
        ))
        .await
        .expect("delete response");
    assert_eq!(delete.status(), StatusCode::OK);
    assert!(
        !harness
            .state
            .app
            .fs
            .exists(&AxiomUri::parse(moved_dir).expect("moved_dir parse"))
    );
}

#[tokio::test]
async fn web_fs_mkdir_rejects_internal_scope() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/fs/mkdir",
            json!({
                "uri": "axiom://queue/editor/newdir"
            }),
        ))
        .await
        .expect("mkdir response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn web_fs_move_rejects_cross_scope_transfer() {
    let harness = TestHarness::setup();
    let response = harness
        .router
        .clone()
        .oneshot(json_request(
            "/api/fs/move",
            json!({
                "from_uri": "axiom://resources/web-editor/guide.md",
                "to_uri": "axiom://user/web-editor/guide.md"
            }),
        ))
        .await
        .expect("move response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload: serde_json::Value = decode_json(response).await;
    assert_eq!(payload["code"], "PERMISSION_DENIED");
}
