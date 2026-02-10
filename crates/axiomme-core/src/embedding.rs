use std::collections::HashSet;
use std::sync::OnceLock;

pub const EMBED_DIM: usize = 64;
pub const EMBEDDER_ENV: &str = "AXIOMME_EMBEDDER";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedderKind {
    SemanticLite,
    Hash,
}

impl EmbedderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SemanticLite => "semantic-lite",
            Self::Hash => "hash",
        }
    }
}

pub fn resolve_embedder_kind(raw: Option<&str>) -> EmbedderKind {
    match raw.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "semantic" || value == "semantic-lite" => {
            EmbedderKind::SemanticLite
        }
        Some(value) if value == "hash" || value == "deterministic" => EmbedderKind::Hash,
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
    fn vector_version(&self) -> &'static str;
    fn embed(&self, text: &str) -> Vec<f32>;
}

#[derive(Debug, Default)]
pub struct HashEmbedder;

impl Embedder for HashEmbedder {
    fn provider(&self) -> &'static str {
        "hash"
    }

    fn vector_version(&self) -> &'static str {
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

    fn vector_version(&self) -> &'static str {
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

static ACTIVE_EMBEDDER: OnceLock<Box<dyn Embedder>> = OnceLock::new();

pub fn embed_text(text: &str) -> Vec<f32> {
    active_embedder().embed(text)
}

pub fn embedding_profile() -> EmbeddingProfile {
    let embedder = active_embedder();
    EmbeddingProfile {
        provider: embedder.provider().to_string(),
        vector_version: embedder.vector_version().to_string(),
        dim: EMBED_DIM,
    }
}

pub fn tokenize_vec(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

pub fn tokenize_set(text: &str) -> HashSet<String> {
    tokenize_vec(text).into_iter().collect()
}

fn active_embedder() -> &'static dyn Embedder {
    ACTIVE_EMBEDDER
        .get_or_init(|| {
            let kind = resolve_embedder_kind(std::env::var(EMBEDDER_ENV).ok().as_deref());
            match kind {
                EmbedderKind::SemanticLite => Box::new(SemanticLiteEmbedder),
                EmbedderKind::Hash => Box::new(HashEmbedder),
            }
        })
        .as_ref()
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
}
