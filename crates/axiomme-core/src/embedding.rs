use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::Value;

use crate::error::{AxiomError, Result};
use crate::llm_io::parse_local_loopback_endpoint;

pub const EMBED_DIM: usize = 64;
pub const EMBEDDER_ENV: &str = "AXIOMME_EMBEDDER";
pub const EMBEDDER_MODEL_ENDPOINT_ENV: &str = "AXIOMME_EMBEDDER_MODEL_ENDPOINT";
pub const EMBEDDER_MODEL_NAME_ENV: &str = "AXIOMME_EMBEDDER_MODEL_NAME";
pub const EMBEDDER_MODEL_TIMEOUT_MS_ENV: &str = "AXIOMME_EMBEDDER_MODEL_TIMEOUT_MS";
pub const EMBEDDER_STRICT_ENV: &str = "AXIOMME_EMBEDDER_STRICT";

const DEFAULT_MODEL_ENDPOINT: &str = "http://127.0.0.1:11434/api/embeddings";
const DEFAULT_MODEL_NAME: &str = "nomic-embed-text";
const MAX_MODEL_INPUT_CHARS: usize = 16 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EmbedderRuntimeConfig {
    pub(crate) kind: Option<String>,
    pub(crate) model_endpoint: Option<String>,
    pub(crate) model_name: Option<String>,
    pub(crate) model_timeout_ms: Option<u64>,
    pub(crate) strict: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedderKind {
    SemanticLite,
    Hash,
    SemanticModelHttp,
}

impl EmbedderKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SemanticLite => "semantic-lite",
            Self::Hash => "hash",
            Self::SemanticModelHttp => "semantic-model-http",
        }
    }
}

#[must_use]
pub fn resolve_embedder_kind(raw: Option<&str>) -> EmbedderKind {
    match raw.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "semantic" || value == "semantic-lite" => {
            EmbedderKind::SemanticLite
        }
        Some(value) if value == "hash" || value == "deterministic" => EmbedderKind::Hash,
        Some(value)
            if value == "semantic-model"
                || value == "semantic-model-http"
                || value == "model-http"
                || value == "ollama" =>
        {
            EmbedderKind::SemanticModelHttp
        }
        _ => EmbedderKind::SemanticLite,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingProfile {
    pub provider: String,
    pub vector_version: String,
    pub dim: usize,
}

pub trait Embedder: Send + Sync {
    fn provider(&self) -> &'static str;
    fn vector_version(&self) -> &str;
    fn embed(&self, text: &str) -> Vec<f32>;
}

#[derive(Debug, Default)]
pub struct HashEmbedder;

impl Embedder for HashEmbedder {
    fn provider(&self) -> &'static str {
        "hash"
    }

    #[allow(
        clippy::unnecessary_literal_bound,
        reason = "trait contract is `&str` to allow dynamic providers; static implementations still return literals"
    )]
    fn vector_version(&self) -> &str {
        "hash-v1"
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; EMBED_DIM];
        for token in tokenize_vec(text) {
            accumulate_feature(&mut vec, &token, 1.0);
        }
        normalize_vector(&mut vec);
        vec
    }
}

#[derive(Debug, Default)]
pub struct SemanticLiteEmbedder;

impl Embedder for SemanticLiteEmbedder {
    fn provider(&self) -> &'static str {
        "semantic-lite"
    }

    #[allow(
        clippy::unnecessary_literal_bound,
        reason = "trait contract is `&str` to allow dynamic providers; static implementations still return literals"
    )]
    fn vector_version(&self) -> &str {
        "semantic-lite-v1"
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; EMBED_DIM];
        let tokens = tokenize_vec(text)
            .into_iter()
            .map(|token| canonicalize_semantic_token(&token))
            .collect::<Vec<_>>();

        for token in &tokens {
            accumulate_feature(&mut vec, token, 1.0);
            for trigram in char_ngrams(token, 3) {
                accumulate_feature(&mut vec, &format!("tri:{trigram}"), 0.35);
            }
        }

        for pair in tokens.windows(2) {
            let feature = format!("bi:{}_{}", pair[0], pair[1]);
            accumulate_feature(&mut vec, &feature, 0.8);
        }

        normalize_vector(&mut vec);
        vec
    }
}

#[derive(Debug)]
pub struct SemanticModelHttpEmbedder {
    client: Client,
    endpoint: Url,
    model: String,
    version: String,
    strict: bool,
    fallback: SemanticLiteEmbedder,
}

impl SemanticModelHttpEmbedder {
    fn from_config(config: &EmbedderRuntimeConfig) -> std::result::Result<Self, String> {
        let endpoint_raw = config
            .model_endpoint
            .as_deref()
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .unwrap_or(DEFAULT_MODEL_ENDPOINT);
        let endpoint = parse_local_endpoint(endpoint_raw)?;

        let model = config
            .model_name
            .as_deref()
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .unwrap_or(DEFAULT_MODEL_NAME)
            .to_string();
        let timeout_ms = config.model_timeout_ms.unwrap_or(3_000).clamp(100, 60_000);
        let strict = config.strict;
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|err| format!("failed to build model embedder client: {err}"))?;
        let version = format!("semantic-model-http:{}:{}", model, endpoint.path());

        Ok(Self {
            client,
            endpoint,
            model,
            version,
            strict,
            fallback: SemanticLiteEmbedder,
        })
    }
}

impl Embedder for SemanticModelHttpEmbedder {
    fn provider(&self) -> &'static str {
        "semantic-model-http"
    }

    fn vector_version(&self) -> &str {
        &self.version
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return vec![0.0; EMBED_DIM];
        }
        let bounded = if trimmed.chars().count() > MAX_MODEL_INPUT_CHARS {
            trimmed
                .chars()
                .take(MAX_MODEL_INPUT_CHARS)
                .collect::<String>()
        } else {
            trimmed.to_string()
        };

        let payload = serde_json::json!({
            "model": self.model,
            "input": bounded,
            "prompt": bounded,
        });
        let response = self
            .client
            .post(self.endpoint.clone())
            .json(&payload)
            .send();
        let Ok(response) = response else {
            if self.strict {
                record_strict_embedder_error("semantic-model-http embed request failed");
            }
            return self.fallback.embed(trimmed);
        };
        if !response.status().is_success() {
            if self.strict {
                record_strict_embedder_error("semantic-model-http non-success status");
            }
            return self.fallback.embed(trimmed);
        }
        let value = response.json::<Value>();
        let Ok(value) = value else {
            if self.strict {
                record_strict_embedder_error("semantic-model-http invalid json response");
            }
            return self.fallback.embed(trimmed);
        };
        let Some(raw) = extract_embedding_vector(&value) else {
            if self.strict {
                record_strict_embedder_error("semantic-model-http missing embedding vector");
            }
            return self.fallback.embed(trimmed);
        };

        project_embedding_to_dim(&raw, EMBED_DIM)
    }
}

static ACTIVE_EMBEDDER: OnceLock<Box<dyn Embedder>> = OnceLock::new();
static EMBEDDER_RUNTIME_CONFIG: OnceLock<EmbedderRuntimeConfig> = OnceLock::new();
static STRICT_EMBEDDER_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub(crate) fn configure_runtime(config: EmbedderRuntimeConfig) -> Result<()> {
    if let Some(existing) = EMBEDDER_RUNTIME_CONFIG.get() {
        return validate_runtime_config(existing, &config);
    }

    if ACTIVE_EMBEDDER.get().is_some() {
        let existing = EMBEDDER_RUNTIME_CONFIG.get_or_init(EmbedderRuntimeConfig::default);
        return validate_runtime_config(existing, &config);
    }

    let _ = EMBEDDER_RUNTIME_CONFIG.set(config);
    Ok(())
}

fn validate_runtime_config(
    existing: &EmbedderRuntimeConfig,
    proposed: &EmbedderRuntimeConfig,
) -> Result<()> {
    if existing == proposed {
        return Ok(());
    }
    Err(AxiomError::Validation(
        "embedding runtime config conflict: already initialized with different values".to_string(),
    ))
}

#[must_use]
pub fn embed_text(text: &str) -> Vec<f32> {
    active_embedder().embed(text)
}

#[must_use]
pub fn embedding_profile() -> EmbeddingProfile {
    let embedder = active_embedder();
    EmbeddingProfile {
        provider: embedder.provider().to_string(),
        vector_version: embedder.vector_version().to_string(),
        dim: EMBED_DIM,
    }
}

#[must_use]
pub fn embedding_strict_mode() -> bool {
    EMBEDDER_RUNTIME_CONFIG
        .get_or_init(EmbedderRuntimeConfig::default)
        .strict
}

pub fn embedding_strict_error() -> Option<String> {
    STRICT_EMBEDDER_ERROR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
}

pub(crate) fn clear_embedding_strict_error() {
    let lock = STRICT_EMBEDDER_ERROR.get_or_init(|| Mutex::new(None));
    if let Ok(mut slot) = lock.lock() {
        *slot = None;
    }
}

#[must_use]
pub fn tokenize_vec(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| !x.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[must_use]
pub fn tokenize_set(text: &str) -> HashSet<String> {
    tokenize_vec(text).into_iter().collect()
}

fn active_embedder() -> &'static dyn Embedder {
    ACTIVE_EMBEDDER
        .get_or_init(|| {
            let runtime = EMBEDDER_RUNTIME_CONFIG.get_or_init(EmbedderRuntimeConfig::default);
            let kind = resolve_embedder_kind(runtime.kind.as_deref());
            match kind {
                EmbedderKind::SemanticLite => Box::new(SemanticLiteEmbedder),
                EmbedderKind::Hash => Box::new(HashEmbedder),
                EmbedderKind::SemanticModelHttp => {
                    match SemanticModelHttpEmbedder::from_config(runtime) {
                        Ok(embedder) => Box::new(embedder),
                        Err(err) => {
                            if embedding_strict_mode() {
                                record_strict_embedder_error(&format!(
                                    "semantic-model-http initialization failed: {err}"
                                ));
                            }
                            Box::new(SemanticLiteEmbedder)
                        }
                    }
                }
            }
        })
        .as_ref()
}

fn record_strict_embedder_error(message: &str) {
    let lock = STRICT_EMBEDDER_ERROR.get_or_init(|| Mutex::new(None));
    if let Ok(mut slot) = lock.lock()
        && slot.is_none()
    {
        *slot = Some(message.to_string());
    }
}

#[cfg(test)]
fn reset_strict_embedder_error_for_tests() {
    clear_embedding_strict_error();
}

fn parse_local_endpoint(raw: &str) -> std::result::Result<Url, String> {
    parse_local_loopback_endpoint(raw, "model endpoint", "local/offline host")
}

fn extract_embedding_vector(value: &Value) -> Option<Vec<f32>> {
    if let Some(values) = value.get("embedding").and_then(|x| x.as_array()) {
        return parse_embedding_values(values);
    }
    if let Some(values) = value
        .get("data")
        .and_then(|x| x.as_array())
        .and_then(|data| data.first())
        .and_then(|first| first.get("embedding"))
        .and_then(|x| x.as_array())
    {
        return parse_embedding_values(values);
    }
    None
}

fn parse_embedding_values(values: &[Value]) -> Option<Vec<f32>> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        let num = value.as_f64()?;
        if !num.is_finite() {
            return None;
        }
        let number = finite_f64_to_f32(num)?;
        out.push(number);
    }
    if out.is_empty() { None } else { Some(out) }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "value bounds are checked against f32::MIN/MAX before casting"
)]
fn finite_f64_to_f32(value: f64) -> Option<f32> {
    let min = f64::from(f32::MIN);
    let max = f64::from(f32::MAX);
    if value < min || value > max {
        return None;
    }
    Some(value as f32)
}

fn project_embedding_to_dim(raw: &[f32], dim: usize) -> Vec<f32> {
    if raw.is_empty() {
        return vec![0.0; dim];
    }
    let mut out = vec![0.0f32; dim];
    for (idx, value) in raw.iter().enumerate() {
        let bucket = idx % dim;
        let sign = if (idx / dim).is_multiple_of(2) {
            1.0
        } else {
            -0.5
        };
        out[bucket] += value * sign;
    }
    normalize_vector(&mut out);
    out
}

fn accumulate_feature(vec: &mut [f32], feature: &str, weight: f32) {
    let hash = blake3::hash(feature.as_bytes());
    let bytes = hash.as_bytes();
    let idx = ((bytes[0] as usize) << 8 | bytes[1] as usize) % EMBED_DIM;
    vec[idx] += weight;
}

fn normalize_vector(vec: &mut [f32]) {
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in vec {
            *value /= norm;
        }
    }
}

fn canonicalize_semantic_token(token: &str) -> String {
    let normalized = token.trim();
    if normalized.is_empty() {
        return String::new();
    }

    let canonical = match normalized {
        "auth" | "oauth" | "authenticate" | "authentication" | "authorize" | "authorization"
        | "login" | "signin" | "credential" | "token" => "identity",
        "storage" | "store" | "cache" | "cached" | "database" | "db" | "persist"
        | "persistence" => "storage",
        "error" | "errors" | "failure" | "fail" | "failed" | "panic" | "incident" => "failure",
        "latency" | "throughput" | "performance" | "slow" | "fast" => "performance",
        _ => normalized,
    };

    stem_suffix(canonical)
}

fn stem_suffix(token: &str) -> String {
    if token.len() <= 4 {
        return token.to_string();
    }

    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(stripped) = token.strip_suffix(suffix)
            && stripped.len() >= 3
        {
            return stripped.to_string();
        }
    }

    token.to_string()
}

fn char_ngrams(token: &str, n: usize) -> Vec<String> {
    if token.chars().count() < n {
        return vec![token.to_string()];
    }

    let chars = token.chars().collect::<Vec<_>>();
    let mut out = Vec::new();
    for i in 0..=chars.len() - n {
        out.push(chars[i..i + n].iter().collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn hash_embedding_is_fixed_dimension() {
        let vec = HashEmbedder.embed("oauth auth flow");
        assert_eq!(vec.len(), EMBED_DIM);
    }

    #[test]
    fn semantic_embedding_is_fixed_dimension() {
        let vec = SemanticLiteEmbedder.embed("oauth auth flow");
        assert_eq!(vec.len(), EMBED_DIM);
    }

    #[test]
    fn tokenization_is_lowercase_and_split() {
        let tokens = tokenize_set("OAuth-flow, API");
        assert!(tokens.contains("oauth"));
        assert!(tokens.contains("flow"));
        assert!(tokens.contains("api"));
    }

    #[test]
    fn semantic_embedder_aligns_auth_synonyms() {
        let semantic = SemanticLiteEmbedder;
        let a = semantic.embed("oauth login flow");
        let b = semantic.embed("authentication signin flow");
        assert!(cosine(&a, &b) > 0.5);
    }

    #[test]
    fn resolve_embedder_kind_defaults_to_semantic_lite() {
        assert_eq!(resolve_embedder_kind(None), EmbedderKind::SemanticLite);
        assert_eq!(
            resolve_embedder_kind(Some("unknown")),
            EmbedderKind::SemanticLite
        );
        assert_eq!(
            resolve_embedder_kind(Some("semantic")),
            EmbedderKind::SemanticLite
        );
        assert_eq!(resolve_embedder_kind(Some("hash")), EmbedderKind::Hash);
        assert_eq!(
            resolve_embedder_kind(Some("semantic-model-http")),
            EmbedderKind::SemanticModelHttp
        );
    }

    #[test]
    fn parse_local_endpoint_rejects_non_local_host() {
        let err = parse_local_endpoint("http://example.com/embed").expect_err("must reject");
        assert!(err.contains("local/offline"));
    }

    #[test]
    fn parse_local_endpoint_accepts_loopback_hosts() {
        assert!(parse_local_endpoint("http://127.0.0.1:11434/api/embeddings").is_ok());
        assert!(parse_local_endpoint("http://localhost:9000/embed").is_ok());
    }

    #[test]
    fn extract_embedding_vector_supports_common_shapes() {
        let ollama = serde_json::json!({
            "embedding": [0.1, -0.2, 0.3]
        });
        let openai = serde_json::json!({
            "data": [{"embedding": [0.4, 0.5, 0.6]}]
        });
        assert_eq!(
            extract_embedding_vector(&ollama).expect("ollama"),
            vec![0.1, -0.2, 0.3]
        );
        assert_eq!(
            extract_embedding_vector(&openai).expect("openai"),
            vec![0.4, 0.5, 0.6]
        );
    }

    #[test]
    fn project_embedding_to_dim_is_fixed_size() {
        let raw = vec![0.1f32; 384];
        let projected = project_embedding_to_dim(&raw, EMBED_DIM);
        assert_eq!(projected.len(), EMBED_DIM);
        let norm = projected.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3 || norm == 0.0);
    }

    #[test]
    fn record_strict_embedder_error_keeps_first_reason() {
        let _guard = strict_test_guard();
        reset_strict_embedder_error_for_tests();
        record_strict_embedder_error("first");
        record_strict_embedder_error("second");
        let reason = embedding_strict_error().expect("reason");
        assert_eq!(reason, "first");
        reset_strict_embedder_error_for_tests();
    }

    #[test]
    fn semantic_model_http_embedder_records_strict_error_on_request_failure() {
        let _guard = strict_test_guard();
        reset_strict_embedder_error_for_tests();
        let embedder = SemanticModelHttpEmbedder {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .expect("client"),
            endpoint: parse_local_endpoint("http://127.0.0.1:1/api/embeddings").expect("endpoint"),
            model: "nomic-embed-text".to_string(),
            version: "semantic-model-http:test".to_string(),
            strict: true,
            fallback: SemanticLiteEmbedder,
        };

        let vector = embedder.embed("oauth login flow");
        assert_eq!(vector.len(), EMBED_DIM);
        let reason = embedding_strict_error().expect("strict reason");
        assert!(reason.contains("embed request failed"));
        reset_strict_embedder_error_for_tests();
    }

    #[test]
    fn semantic_model_http_embedder_records_strict_error_on_non_success_status() {
        let _guard = strict_test_guard();
        reset_strict_embedder_error_for_tests();
        let (endpoint, handle) = match spawn_single_response_server(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\n\r\n{}",
        ) {
            Ok(server) => server,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => return,
            Err(err) => panic!("failed to spawn local test server: {err}"),
        };
        let embedder = SemanticModelHttpEmbedder {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .expect("client"),
            endpoint: parse_local_endpoint(&endpoint).expect("endpoint"),
            model: "nomic-embed-text".to_string(),
            version: "semantic-model-http:test".to_string(),
            strict: true,
            fallback: SemanticLiteEmbedder,
        };

        let vector = embedder.embed("oauth login flow");
        assert_eq!(vector.len(), EMBED_DIM);
        let reason = embedding_strict_error().expect("strict reason");
        assert!(reason.contains("non-success status"));
        handle.join().expect("server join");
        reset_strict_embedder_error_for_tests();
    }

    #[test]
    fn semantic_model_http_embedder_uses_server_embedding_on_success() {
        let _guard = strict_test_guard();
        reset_strict_embedder_error_for_tests();
        let body = r#"{"embedding":[0.1,0.2,0.3,0.4]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let (endpoint, handle) = match spawn_single_response_server(&response) {
            Ok(server) => server,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => return,
            Err(err) => panic!("failed to spawn local test server: {err}"),
        };
        let embedder = SemanticModelHttpEmbedder {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .expect("client"),
            endpoint: parse_local_endpoint(&endpoint).expect("endpoint"),
            model: "nomic-embed-text".to_string(),
            version: "semantic-model-http:test".to_string(),
            strict: true,
            fallback: SemanticLiteEmbedder,
        };

        let vector = embedder.embed("oauth login flow");
        assert_eq!(vector.len(), EMBED_DIM);
        // Successful server response must not set strict error state.
        assert!(embedding_strict_error().is_none());
        handle.join().expect("server join");
        reset_strict_embedder_error_for_tests();
    }

    #[test]
    fn semantic_embedding_performance_smoke() {
        let semantic = SemanticLiteEmbedder;
        let corpus = [
            "OAuth login flow and token refresh",
            "database storage cache invalidation guide",
            "incident response failure postmortem",
            "performance latency throughput baseline",
            "authentication authorization identity provider",
        ];

        let started = Instant::now();
        let mut checksum = 0.0f32;
        for i in 0..4_000 {
            let vec = semantic.embed(corpus[i % corpus.len()]);
            checksum += vec[i % EMBED_DIM];
        }
        let elapsed = started.elapsed();

        assert!(checksum.is_finite());
        assert!(elapsed < Duration::from_secs(3));
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        let len = a.len().min(b.len());
        let mut sum = 0.0;
        for i in 0..len {
            sum += a[i] * b[i];
        }
        sum
    }

    fn spawn_single_response_server(
        response: &str,
    ) -> std::io::Result<(String, thread::JoinHandle<()>)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let payload = response.as_bytes().to_vec();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(&payload);
                let _ = stream.flush();
            }
        });
        Ok((format!("http://{addr}/api/embeddings"), handle))
    }

    fn strict_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
