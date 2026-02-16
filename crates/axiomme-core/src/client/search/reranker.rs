use crate::error::{AxiomError, Result};
use crate::models::{ContextHit, FindResult, IndexRecord};

use super::AxiomMe;
use super::result::{append_query_plan_note, split_hits, sync_trace_final_topk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RerankerMode {
    Off,
    DocAwareV1,
}

impl RerankerMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::DocAwareV1 => "doc-aware-v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryIntent {
    Lexical,
    Semantic,
    Mixed,
}

impl QueryIntent {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Semantic => "semantic",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentClass {
    Code,
    Config,
    Spec,
    Narrative,
    Memory,
    Skill,
    Session,
    Data,
    General,
}

pub(super) fn resolve_reranker_mode(raw: Option<&str>) -> RerankerMode {
    if raw.is_some_and(|value| {
        let value = value.trim();
        value.eq_ignore_ascii_case("off")
            || value.eq_ignore_ascii_case("none")
            || value.eq_ignore_ascii_case("disabled")
    }) {
        RerankerMode::Off
    } else {
        RerankerMode::DocAwareV1
    }
}

fn classify_query_intent(query: &str, tokens: &[String]) -> QueryIntent {
    let has_symbolic = query.contains("::")
        || query.contains('/')
        || query.contains('.')
        || query.contains('_')
        || query.contains('`');
    let has_digit = query.chars().any(|ch| ch.is_ascii_digit());
    if (has_symbolic || has_digit) && tokens.len() <= 3 {
        return QueryIntent::Lexical;
    }
    if tokens.len() >= 4 && !has_symbolic && !has_digit {
        return QueryIntent::Semantic;
    }
    QueryIntent::Mixed
}

fn query_has_any(tokens: &[String], terms: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| terms.iter().any(|term| token == term))
}

fn classify_document_class(hit: &ContextHit, record: Option<&IndexRecord>) -> DocumentClass {
    if hit.context_type == "memory" {
        return DocumentClass::Memory;
    }
    if hit.context_type == "skill" {
        return DocumentClass::Skill;
    }
    if hit.context_type == "session" || hit.uri.starts_with("axiom://session/") {
        return DocumentClass::Session;
    }

    let (name, uri_lower) = lowercased_name_and_uri(hit, record);
    let ext = name.rsplit('.').next().unwrap_or_default();

    if uri_lower.contains("/spec")
        || uri_lower.contains("/contract")
        || uri_lower.contains("/openapi")
        || uri_lower.contains("/schema")
        || name.contains("openapi")
        || name.contains("schema")
        || name.contains("contract")
    {
        return DocumentClass::Spec;
    }
    if matches!(
        ext,
        "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "java" | "go" | "c" | "cpp" | "h"
    ) {
        return DocumentClass::Code;
    }
    if matches!(
        ext,
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "env"
    ) {
        return DocumentClass::Config;
    }
    if matches!(ext, "json" | "jsonl" | "csv" | "tsv" | "parquet") {
        return DocumentClass::Data;
    }
    if matches!(ext, "md" | "markdown" | "txt" | "rst" | "adoc") {
        return DocumentClass::Narrative;
    }

    DocumentClass::General
}

#[derive(Debug, Clone, Copy, Default)]
struct QueryNeeds(u8);

impl QueryNeeds {
    const API: u8 = 1 << 0;
    const CONFIG: u8 = 1 << 1;
    const CODE: u8 = 1 << 2;
    const GUIDE: u8 = 1 << 3;
    const MEMORY: u8 = 1 << 4;
    const SKILL: u8 = 1 << 5;
    const SESSION: u8 = 1 << 6;

    const fn insert_if(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        }
    }

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

fn detect_query_needs(query_tokens: &[String], intent: QueryIntent) -> QueryNeeds {
    let mut needs = QueryNeeds::default();
    needs.insert_if(
        QueryNeeds::API,
        query_has_any(
            query_tokens,
            &[
                "api", "endpoint", "schema", "contract", "spec", "openapi", "grpc",
            ],
        ),
    );
    needs.insert_if(
        QueryNeeds::CONFIG,
        query_has_any(
            query_tokens,
            &[
                "config",
                "configuration",
                "setting",
                "settings",
                "env",
                "yaml",
                "yml",
                "toml",
                "json",
                "ini",
                "docker",
                "compose",
            ],
        ),
    );
    needs.insert_if(
        QueryNeeds::CODE,
        matches!(intent, QueryIntent::Lexical)
            || query_has_any(
                query_tokens,
                &[
                    "code", "impl", "function", "stack", "panic", "compile", "build", "trace",
                ],
            ),
    );
    needs.insert_if(
        QueryNeeds::GUIDE,
        query_has_any(
            query_tokens,
            &["guide", "overview", "summary", "explain", "how", "why"],
        ),
    );
    needs.insert_if(
        QueryNeeds::MEMORY,
        query_has_any(
            query_tokens,
            &["memory", "memories", "preference", "remember"],
        ),
    );
    needs.insert_if(
        QueryNeeds::SKILL,
        query_has_any(query_tokens, &["skill", "skills", "tool", "tools"]),
    );
    needs.insert_if(
        QueryNeeds::SESSION,
        query_has_any(query_tokens, &["session", "recent", "conversation", "chat"]),
    );
    needs
}

fn lowercased_name_and_uri(hit: &ContextHit, record: Option<&IndexRecord>) -> (String, String) {
    record.map_or_else(
        || {
            let name = hit
                .uri
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            (name, hit.uri.to_ascii_lowercase())
        },
        |record| {
            (
                record.name.to_ascii_lowercase(),
                record.uri.to_ascii_lowercase(),
            )
        },
    )
}

const fn base_doc_class_boost(intent: QueryIntent, doc_class: DocumentClass) -> f32 {
    match (intent, doc_class) {
        (QueryIntent::Lexical, DocumentClass::Code)
        | (QueryIntent::Semantic, DocumentClass::Narrative) => 0.12,
        (QueryIntent::Lexical, DocumentClass::Config)
        | (QueryIntent::Mixed, DocumentClass::Spec) => 0.10,
        (QueryIntent::Semantic, DocumentClass::Spec | DocumentClass::Memory) => 0.09,
        (QueryIntent::Lexical, DocumentClass::Spec)
        | (
            QueryIntent::Mixed,
            DocumentClass::Narrative | DocumentClass::Code | DocumentClass::Config,
        ) => 0.08,
        _ => 0.0,
    }
}

fn query_need_boost(needs: QueryNeeds, doc_class: DocumentClass) -> f32 {
    let mut boost = 0.0;
    if needs.contains(QueryNeeds::API) && matches!(doc_class, DocumentClass::Spec) {
        boost += 0.22;
    }
    if needs.contains(QueryNeeds::CONFIG)
        && matches!(doc_class, DocumentClass::Config | DocumentClass::Data)
    {
        boost += 0.20;
    }
    if needs.contains(QueryNeeds::CODE) && matches!(doc_class, DocumentClass::Code) {
        boost += 0.18;
    }
    if needs.contains(QueryNeeds::GUIDE)
        && matches!(doc_class, DocumentClass::Narrative | DocumentClass::Spec)
    {
        boost += 0.16;
    }
    if needs.contains(QueryNeeds::MEMORY) && matches!(doc_class, DocumentClass::Memory) {
        boost += 0.24;
    }
    if needs.contains(QueryNeeds::SKILL) && matches!(doc_class, DocumentClass::Skill) {
        boost += 0.24;
    }
    if needs.contains(QueryNeeds::SESSION) && matches!(doc_class, DocumentClass::Session) {
        boost += 0.20;
    }
    boost
}

fn uri_or_name_overlap_boost(query_tokens: &[String], name_lower: &str, uri_lower: &str) -> f32 {
    if query_tokens
        .iter()
        .any(|token| name_lower.contains(token) || uri_lower.contains(token))
    {
        0.08
    } else {
        0.0
    }
}

fn tag_overlap_boost(record: Option<&IndexRecord>, query_tokens: &[String]) -> f32 {
    let Some(record) = record else {
        return 0.0;
    };
    let overlap = record
        .tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .filter(|tag| query_tokens.iter().any(|token| token == tag))
        .count()
        .min(3);
    match overlap {
        0 => 0.0,
        1 => 0.03,
        2 => 0.06,
        _ => 0.09,
    }
}

fn doc_aware_boost(
    query_tokens: &[String],
    intent: QueryIntent,
    hit: &ContextHit,
    record: Option<&IndexRecord>,
) -> f32 {
    let doc_class = classify_document_class(hit, record);
    let needs = detect_query_needs(query_tokens, intent);
    let (name_lower, uri_lower) = lowercased_name_and_uri(hit, record);
    let boost = base_doc_class_boost(intent, doc_class)
        + query_need_boost(needs, doc_class)
        + uri_or_name_overlap_boost(query_tokens, &name_lower, &uri_lower)
        + tag_overlap_boost(record, query_tokens);
    boost.clamp(0.0, 0.65)
}

impl AxiomMe {
    pub(super) fn apply_reranker_with_mode(
        &self,
        query: &str,
        result: &mut FindResult,
        limit: usize,
        mode: RerankerMode,
    ) -> Result<()> {
        append_query_plan_note(result, &format!("reranker:{}", mode.as_str()));
        if mode == RerankerMode::Off || result.query_results.len() <= 1 {
            sync_trace_final_topk(result);
            return Ok(());
        }

        let query_tokens = crate::embedding::tokenize_vec(query);
        let intent = classify_query_intent(query, &query_tokens);
        append_query_plan_note(result, &format!("reranker_intent:{}", intent.as_str()));

        let index = self
            .index
            .read()
            .map_err(|_| AxiomError::lock_poisoned("index"))?;
        let mut reranked = result
            .query_results
            .iter()
            .map(|hit| {
                let record = index.get(&hit.uri);
                let boost = doc_aware_boost(&query_tokens, intent, hit, record);
                let mut out = hit.clone();
                out.score = (out.score * (1.0 + boost)).max(0.0);
                out
            })
            .collect::<Vec<_>>();
        drop(index);

        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.uri.cmp(&b.uri))
        });
        reranked.truncate(limit.max(1));
        let (memories, resources, skills) = split_hits(&reranked);
        result.query_results = reranked;
        result.memories = memories;
        result.resources = resources;
        result.skills = skills;
        sync_trace_final_topk(result);

        Ok(())
    }
}
