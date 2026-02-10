use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::json;

use crate::embedding::{EMBED_DIM, embed_text};
use crate::error::{AxiomError, Result};
use crate::models::{IndexRecord, SearchFilter};

pub const VECTOR_DIM: usize = EMBED_DIM;

#[derive(Debug, Clone, PartialEq)]
pub struct QdrantSearchHit {
    pub uri: String,
    pub score: f32,
    pub context_type: String,
    pub abstract_text: String,
    pub category: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct QdrantConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub collection: String,
    pub timeout_ms: u64,
}

impl QdrantConfig {
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("AXIOMME_QDRANT_URL").ok()?;
        let collection = std::env::var("AXIOMME_QDRANT_COLLECTION")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "axiomme_l0".to_string());
        let timeout_ms = std::env::var("AXIOMME_QDRANT_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(2000);

        Some(Self {
            base_url: normalize_base_url(&base_url),
            api_key: std::env::var("AXIOMME_QDRANT_API_KEY").ok(),
            collection,
            timeout_ms,
        })
    }
}

#[derive(Clone)]
pub struct QdrantMirror {
    config: QdrantConfig,
    http: Client,
}

impl std::fmt::Debug for QdrantMirror {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QdrantMirror")
            .field("base_url", &self.config.base_url)
            .field("collection", &self.config.collection)
            .finish_non_exhaustive()
    }
}

impl QdrantMirror {
    pub fn new(config: QdrantConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        if let Some(key) = &config.api_key {
            let value = HeaderValue::from_str(key).map_err(|e| {
                AxiomError::Validation(format!("invalid AXIOMME_QDRANT_API_KEY: {e}"))
            })?;
            headers.insert("api-key", value);
        }

        let http = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()?;

        Ok(Self { config, http })
    }

    pub fn config(&self) -> &QdrantConfig {
        &self.config
    }

    pub fn health(&self) -> Result<bool> {
        let url = format!("{}/collections", self.config.base_url);
        let resp = self.http.get(url).send()?;
        Ok(resp.status().is_success())
    }

    pub fn server_version(&self) -> Result<Option<String>> {
        let url = format!("{}/version", self.config.base_url);
        let resp = self.http.get(url).send()?;
        if !resp.status().is_success() {
            return Ok(None);
        }

        let value = resp.json::<serde_json::Value>()?;
        let version = value
            .pointer("/result/version")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("version").and_then(|v| v.as_str()))
            .or_else(|| {
                value
                    .pointer("/result/build/version")
                    .and_then(|v| v.as_str())
            })
            .map(ToString::to_string);
        Ok(version)
    }

    pub fn ensure_collection(&self) -> Result<()> {
        let exists_url = format!(
            "{}/collections/{}",
            self.config.base_url, self.config.collection
        );
        let exists = self.http.get(&exists_url).send()?;
        if exists.status().is_success() {
            return Ok(());
        }
        if exists.status().as_u16() != 404 {
            return Err(AxiomError::Internal(format!(
                "qdrant collection check failed with status {}",
                exists.status()
            )));
        }

        let create_url = exists_url;
        let body = json!({
            "vectors": {
                "size": VECTOR_DIM,
                "distance": "Cosine"
            }
        });
        let created = self.http.put(create_url).json(&body).send()?;
        if !created.status().is_success() {
            return Err(AxiomError::Internal(format!(
                "qdrant collection create failed with status {}",
                created.status()
            )));
        }
        Ok(())
    }

    pub fn upsert_record(&self, record: &IndexRecord) -> Result<()> {
        let id = point_id_for_uri(&record.uri);
        let vector = embed_text_for_qdrant(record);
        let payload = payload_for_record(record);

        let body = json!({
            "points": [{
                "id": id,
                "vector": vector,
                "payload": payload
            }]
        });

        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.config.base_url, self.config.collection
        );

        let resp = self.http.put(url).json(&body).send()?;
        if !resp.status().is_success() {
            return Err(AxiomError::Internal(format!(
                "qdrant upsert failed for {} with status {}",
                record.uri,
                resp.status()
            )));
        }
        Ok(())
    }

    pub fn delete_uris(&self, uris: &[String]) -> Result<()> {
        if uris.is_empty() {
            return Ok(());
        }

        let points = uris
            .iter()
            .map(|uri| point_id_for_uri(uri))
            .collect::<Vec<_>>();

        let body = json!({
            "points": points,
        });

        let url = format!(
            "{}/collections/{}/points/delete?wait=true",
            self.config.base_url, self.config.collection
        );

        let resp = self.http.post(url).json(&body).send()?;
        if !resp.status().is_success() {
            return Err(AxiomError::Internal(format!(
                "qdrant delete failed with status {}",
                resp.status()
            )));
        }

        Ok(())
    }

    pub fn search_points(
        &self,
        query: &str,
        limit: usize,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<QdrantSearchHit>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let vector = embed_text(query);
        let body = build_search_points_request(&vector, limit, filter);
        let url = format!(
            "{}/collections/{}/points/search",
            self.config.base_url, self.config.collection
        );
        let resp = self.http.post(url).json(&body).send()?;
        if !resp.status().is_success() {
            return Err(AxiomError::Internal(format!(
                "qdrant search failed with status {}",
                resp.status()
            )));
        }

        let value = resp.json::<serde_json::Value>()?;
        parse_search_points_response(&value)
    }
}

pub(crate) fn point_id_for_uri(uri: &str) -> u64 {
    let hash = blake3::hash(uri.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_be_bytes(bytes)
}

pub(crate) fn payload_for_record(record: &IndexRecord) -> serde_json::Value {
    let profile = crate::embedding::embedding_profile();
    json!({
        "uri": record.uri,
        "parent_uri": record.parent_uri,
        "is_leaf": record.is_leaf,
        "context_type": record.context_type,
        "category": payload_category(record),
        "name": record.name,
        "abstract": record.abstract_text,
        "tags": record.tags,
        "mime": payload_mime(record),
        "updated_at": record.updated_at.to_rfc3339(),
        "depth": record.depth,
        "vector_provider": profile.provider,
        "vector_version": profile.vector_version,
        "vector_dim": profile.dim,
    })
}

pub(crate) fn build_search_points_request(
    vector: &[f32],
    limit: usize,
    filter: Option<&SearchFilter>,
) -> serde_json::Value {
    let mut body = json!({
        "vector": vector,
        "limit": limit.max(1),
        "with_payload": true,
    });
    if let Some(value) = search_filter_to_qdrant_filter(filter) {
        body["filter"] = value;
    }
    body
}

pub(crate) fn search_filter_to_qdrant_filter(
    filter: Option<&SearchFilter>,
) -> Option<serde_json::Value> {
    let filter = filter?;
    let mut must = Vec::<serde_json::Value>::new();

    for tag in filter
        .tags
        .iter()
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
    {
        must.push(json!({
            "key": "tags",
            "match": {"value": tag}
        }));
    }

    if let Some(mime) = filter
        .mime
        .as_ref()
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
    {
        must.push(json!({
            "key": "mime",
            "match": {"value": mime}
        }));
    }

    if must.is_empty() {
        return None;
    }
    Some(json!({ "must": must }))
}

pub(crate) fn parse_search_points_response(
    response: &serde_json::Value,
) -> Result<Vec<QdrantSearchHit>> {
    let points = response
        .get("result")
        .and_then(|value| value.as_array())
        .ok_or_else(|| AxiomError::Validation("invalid qdrant search response".to_string()))?;

    let mut hits = Vec::<QdrantSearchHit>::new();
    for point in points {
        let payload = point.get("payload").and_then(|value| value.as_object());
        let Some(payload) = payload else {
            continue;
        };
        let Some(uri) = payload.get("uri").and_then(|value| value.as_str()) else {
            continue;
        };
        let score = point
            .get("score")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0) as f32;
        let context_type = payload
            .get("context_type")
            .and_then(|value| value.as_str())
            .unwrap_or("resource")
            .to_string();
        let abstract_text = payload
            .get("abstract")
            .or_else(|| payload.get("abstract_text"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let category = payload
            .get("category")
            .and_then(|value| value.as_str())
            .unwrap_or("resource")
            .to_string();
        let tags = payload
            .get("tags")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        hits.push(QdrantSearchHit {
            uri: uri.to_string(),
            score,
            context_type,
            abstract_text,
            category,
            tags,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.uri.cmp(&b.uri))
    });
    Ok(hits)
}

pub(crate) fn embed_text_for_qdrant(record: &IndexRecord) -> Vec<f32> {
    let text = [
        record.name.as_str(),
        record.abstract_text.as_str(),
        record.content.as_str(),
        &record.tags.join(" "),
    ]
    .join(" ");
    embed_text(&text)
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn payload_category(record: &IndexRecord) -> String {
    let uri = record.uri.as_str();
    if uri.contains("/memories/profile.md") {
        return "profile".to_string();
    }
    if uri.contains("/memories/preferences/") {
        return "preferences".to_string();
    }
    if uri.contains("/memories/entities/") {
        return "entities".to_string();
    }
    if uri.contains("/memories/events/") {
        return "events".to_string();
    }
    if uri.contains("/memories/cases/") {
        return "cases".to_string();
    }
    if uri.contains("/memories/patterns/") {
        return "patterns".to_string();
    }
    if uri.starts_with("axiom://agent/skills/") {
        return "skill".to_string();
    }
    if uri.starts_with("axiom://session/") {
        return "session".to_string();
    }
    if record.context_type == "memory" {
        return "memory".to_string();
    }
    if record.context_type == "skill" {
        return "skill".to_string();
    }
    "resource".to_string()
}

fn payload_mime(record: &IndexRecord) -> Option<&'static str> {
    if !record.is_leaf {
        return None;
    }
    infer_mime_from_name(&record.name)
}

fn infer_mime_from_name(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("text/markdown"),
        "txt" | "log" => Some("text/plain"),
        "json" => Some("application/json"),
        "rs" => Some("text/rust"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::models::SearchFilter;

    fn sample_record(uri: &str) -> IndexRecord {
        IndexRecord {
            id: "1".to_string(),
            uri: uri.to_string(),
            parent_uri: Some("axiom://resources/demo".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "doc.md".to_string(),
            abstract_text: "oauth flow".to_string(),
            content: "authorization code".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        }
    }

    #[test]
    fn point_id_is_deterministic() {
        let a = point_id_for_uri("axiom://resources/demo/doc.md");
        let b = point_id_for_uri("axiom://resources/demo/doc.md");
        assert_eq!(a, b);
    }

    #[test]
    fn vector_has_fixed_dim() {
        let record = sample_record("axiom://resources/demo/doc.md");
        let vec = embed_text_for_qdrant(&record);
        assert_eq!(vec.len(), VECTOR_DIM);
    }

    #[test]
    fn payload_contains_uri_and_context() {
        let record = sample_record("axiom://resources/demo/doc.md");
        let payload = payload_for_record(&record);
        let profile = crate::embedding::embedding_profile();
        assert_eq!(payload["uri"], record.uri);
        assert_eq!(payload["context_type"], record.context_type);
        assert_eq!(payload["is_leaf"], record.is_leaf);
        assert_eq!(payload["category"], "resource");
        assert_eq!(payload["mime"], "text/markdown");
        assert_eq!(payload["vector_provider"], profile.provider);
        assert_eq!(payload["vector_version"], profile.vector_version);
        assert_eq!(payload["vector_dim"], VECTOR_DIM);
    }

    #[test]
    fn payload_category_marks_memory_subtypes() {
        let mut record = sample_record("axiom://user/memories/preferences/pref-rust.md");
        record.context_type = "memory".to_string();
        let payload = payload_for_record(&record);
        assert_eq!(payload["category"], "preferences");
    }

    #[test]
    fn build_search_points_request_includes_filter_clauses() {
        let filter = SearchFilter {
            tags: vec!["auth".to_string(), "api".to_string()],
            mime: Some("text/markdown".to_string()),
        };
        let body = build_search_points_request(&vec![0.1; VECTOR_DIM], 7, Some(&filter));
        assert_eq!(body["limit"], 7);
        assert_eq!(body["with_payload"], true);
        let must = body["filter"]["must"].as_array().expect("must");
        assert_eq!(must.len(), 3);
        assert_eq!(must[0]["key"], "tags");
        assert_eq!(must[1]["key"], "tags");
        assert_eq!(must[2]["key"], "mime");
    }

    #[test]
    fn parse_search_points_response_extracts_ranked_hits() {
        let response = json!({
            "result": [
                {
                    "score": 0.91,
                    "payload": {
                        "uri": "axiom://resources/demo/auth.md",
                        "context_type": "resource",
                        "abstract": "OAuth flow",
                        "category": "resource",
                        "tags": ["auth", "api"]
                    }
                },
                {
                    "score": 0.88,
                    "payload": {
                        "uri": "axiom://resources/demo/storage.md",
                        "context_type": "resource",
                        "abstract": "Storage guide",
                        "category": "resource",
                        "tags": ["storage"]
                    }
                }
            ]
        });

        let hits = parse_search_points_response(&response).expect("parse");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].uri, "axiom://resources/demo/auth.md");
        assert_eq!(hits[0].category, "resource");
        assert_eq!(hits[0].tags, vec!["auth".to_string(), "api".to_string()]);
    }

    #[test]
    fn parse_search_points_response_skips_invalid_payload_entries() {
        let response = json!({
            "result": [
                {"score": 0.9, "payload": {"context_type": "resource"}},
                {"score": 0.8, "payload": {"uri": "axiom://resources/demo/ok.md", "context_type": "resource"}}
            ]
        });

        let hits = parse_search_points_response(&response).expect("parse");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].uri, "axiom://resources/demo/ok.md");
    }
}
