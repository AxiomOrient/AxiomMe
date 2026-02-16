use std::fs;

use axum::{
    Router,
    body::{Body, to_bytes},
    response::Response,
};

use axiomme_core::AxiomMe;

use crate::{WebState, app_router};

pub(super) struct TestHarness {
    _temp: tempfile::TempDir,
    pub(super) state: WebState,
    pub(super) router: Router,
    pub(super) uri: String,
    pub(super) parent_uri: String,
}

impl TestHarness {
    pub(super) fn setup() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let corpus = temp.path().join("corpus");
        fs::create_dir_all(&corpus).expect("mkdir corpus");
        fs::write(corpus.join("guide.md"), "# Guide\n\nalpha_token").expect("seed markdown");
        fs::write(corpus.join("config.json"), "{\"name\":\"axiomme\",\"v\":1}").expect("seed json");
        fs::write(corpus.join("policy.yaml"), "mode: strict\nenabled: true\n").expect("seed yaml");
        fs::write(
            corpus.join("events.jsonl"),
            "{\"t\":\"a\"}\n{\"t\":\"b\"}\n",
        )
        .expect("seed jsonl");
        fs::write(corpus.join("layout.xml"), "<root><item>v</item></root>").expect("seed xml");
        fs::write(corpus.join("script.sh"), "echo hi\n").expect("seed script");

        app.add_resource(
            corpus.to_str().expect("corpus str"),
            Some("axiom://resources/web-editor"),
            None,
            None,
            true,
            None,
        )
        .expect("add resource");

        let state = WebState::new(app);
        let router = app_router(state.clone());
        Self {
            _temp: temp,
            state,
            router,
            uri: "axiom://resources/web-editor/guide.md".to_string(),
            parent_uri: "axiom://resources/web-editor".to_string(),
        }
    }
}

pub(super) async fn decode_json<T: serde::de::DeserializeOwned>(response: Response) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    serde_json::from_slice(&bytes).expect("decode json")
}

pub(super) fn header_value<'a>(headers: &'a axum::http::HeaderMap, key: &str) -> Option<&'a str> {
    headers.get(key).and_then(|value| value.to_str().ok())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "tests usually pass temporary `json!` values directly"
)]
pub(super) fn json_request(path: &str, body: serde_json::Value) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&body).expect("json request body"),
        ))
        .expect("json request")
}
