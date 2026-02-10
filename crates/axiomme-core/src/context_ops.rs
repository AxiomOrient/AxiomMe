use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;

use crate::catalog::sanitize_component;
use crate::error::{AxiomError, Result};
use crate::models::{IndexRecord, MetadataFilter};
use crate::uri::{AxiomUri, Scope};

pub(crate) fn default_resource_target(path_or_url: &str) -> Result<AxiomUri> {
    let base = if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        let stripped = path_or_url
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        sanitize_component(stripped)
    } else {
        let path = Path::new(path_or_url);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| AxiomError::Validation("cannot infer target name".to_string()))?;
        sanitize_component(name)
    };

    AxiomUri::root(Scope::Resources).join(&base)
}

pub(crate) fn classify_context(uri: &AxiomUri) -> String {
    let uri_str = uri.to_string();
    if uri_str.starts_with("axiom://agent/skills") {
        return "skill".to_string();
    }
    if uri_str.starts_with("axiom://user/memories") || uri_str.starts_with("axiom://agent/memories")
    {
        return "memory".to_string();
    }
    if matches!(uri.scope(), Scope::Session) {
        return "session".to_string();
    }
    "resource".to_string()
}

pub(crate) fn infer_tags(name: &str, content: &str) -> Vec<String> {
    let mut tags = HashSet::new();

    let lower_name = name.to_lowercase();
    if lower_name.ends_with(".rs") {
        tags.insert("rust".to_string());
    }
    if lower_name.ends_with(".md") {
        tags.insert("markdown".to_string());
    }
    if lower_name.ends_with(".json") {
        tags.insert("json".to_string());
    }

    let lower_content = content.to_lowercase();
    for token in ["auth", "oauth", "session", "memory", "skill", "api"] {
        if lower_content.contains(token) {
            tags.insert(token.to_string());
        }
    }

    let mut out: Vec<_> = tags.into_iter().collect();
    out.sort();
    out
}

pub(crate) fn validate_filter(filter: Option<&MetadataFilter>) -> Result<()> {
    let Some(filter) = filter else {
        return Ok(());
    };

    let allowed = ["tags", "mime"];
    for key in filter.fields.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(AxiomError::Validation(format!(
                "unknown filter field: {}",
                key
            )));
        }
    }
    Ok(())
}

pub(crate) struct RecordInput<'a> {
    pub uri: &'a AxiomUri,
    pub parent_uri: Option<&'a AxiomUri>,
    pub is_leaf: bool,
    pub context_type: String,
    pub name: String,
    pub abstract_text: String,
    pub content: String,
    pub tags: Vec<String>,
}

pub(crate) fn build_record(input: RecordInput<'_>) -> IndexRecord {
    IndexRecord {
        id: uuid::Uuid::new_v4().to_string(),
        uri: input.uri.to_string(),
        parent_uri: input.parent_uri.map(|p| p.to_string()),
        is_leaf: input.is_leaf,
        context_type: input.context_type,
        name: input.name,
        abstract_text: input.abstract_text,
        content: input.content,
        tags: input.tags,
        updated_at: Utc::now(),
        depth: input.uri.segments().len(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn validate_filter_rejects_unknown_fields() {
        let mut fields = HashMap::new();
        fields.insert("unknown".to_string(), serde_json::json!("x"));
        let err = validate_filter(Some(&MetadataFilter { fields })).expect_err("must fail");
        assert!(matches!(err, AxiomError::Validation(_)));
    }

    #[test]
    fn classify_context_maps_memory_and_skill_paths() {
        let memory = AxiomUri::parse("axiom://user/memories/preferences/rust.md").expect("parse");
        let skill = AxiomUri::parse("axiom://agent/skills/retrieval.md").expect("parse");
        let session = AxiomUri::parse("axiom://session/s1/messages").expect("parse");
        let resource = AxiomUri::parse("axiom://resources/api/auth.md").expect("parse");

        assert_eq!(classify_context(&memory), "memory");
        assert_eq!(classify_context(&skill), "skill");
        assert_eq!(classify_context(&session), "session");
        assert_eq!(classify_context(&resource), "resource");
    }

    #[test]
    fn infer_tags_extracts_extension_and_keyword_tags() {
        let tags = infer_tags("auth_flow.rs", "OAuth API session memory");
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"oauth".to_string()));
        assert!(tags.contains(&"api".to_string()));
        assert!(tags.contains(&"session".to_string()));
    }

    #[test]
    fn default_resource_target_from_http_url_uses_sanitized_host_path() {
        let uri =
            default_resource_target("https://example.com/Awesome Path").expect("default target");
        assert_eq!(uri.to_string(), "axiom://resources/example-comawesomepath");
    }

    #[test]
    fn build_record_sets_depth_and_parent_uri() {
        let uri = AxiomUri::parse("axiom://resources/demo/node.md").expect("parse");
        let parent = AxiomUri::parse("axiom://resources/demo").expect("parse");
        let record = build_record(RecordInput {
            uri: &uri,
            parent_uri: Some(&parent),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "node.md".to_string(),
            abstract_text: "node".to_string(),
            content: "content".to_string(),
            tags: vec!["markdown".to_string()],
        });
        assert_eq!(record.parent_uri.as_deref(), Some("axiom://resources/demo"));
        assert_eq!(record.depth, 2);
    }
}
