use chrono::Utc;

use crate::index::InMemoryIndex;
use crate::models::{IndexRecord, SearchBudget, SearchFilter, SearchOptions};
use crate::retrieval::{DrrConfig, DrrEngine};
use crate::uri::AxiomUri;

#[test]
fn drr_returns_trace_and_hits() {
    let mut index = InMemoryIndex::new();

    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });

    index.upsert(IndexRecord {
        id: "docs".to_string(),
        uri: "axiom://resources/docs".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "docs".to_string(),
        abstract_text: "documentation".to_string(),
        content: "auth docs".to_string(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 1,
    });

    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources/docs".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "oauth flow".to_string(),
        content: "oauth authorization code".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "oauth".to_string(),
            target_uri: None,
            session: None,
            session_hints: Vec::new(),
            budget: None,
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "find".to_string(),
        },
    );

    assert!(!result.query_results.is_empty());
    assert!(result.trace.is_some());
}

#[test]
fn search_query_plan_includes_typed_queries() {
    let mut index = InMemoryIndex::new();

    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });

    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "oauth flow".to_string(),
        content: "oauth authorization code".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 1,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "oauth".to_string(),
            target_uri: None,
            session: Some("s-1".to_string()),
            session_hints: vec!["use refresh token".to_string()],
            budget: None,
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "search".to_string(),
        },
    );

    let plan = result.query_plan;
    assert!(plan.typed_queries.iter().any(|x| x.kind == "primary"));
    assert!(
        plan.typed_queries
            .iter()
            .any(|x| x.kind == "session_recent")
    );
    assert!(plan.typed_queries.len() >= 2);
}

#[test]
fn search_query_plan_includes_session_om_query_and_visibility_note() {
    let mut index = InMemoryIndex::new();
    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });
    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "oauth flow".to_string(),
        content: "oauth authorization code".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 1,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "oauth".to_string(),
            target_uri: None,
            session: Some("s-om".to_string()),
            session_hints: vec![
                "recent user hint".to_string(),
                "om: compact long-term memory".to_string(),
            ],
            budget: None,
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "search".to_string(),
        },
    );

    let plan = result.query_plan;
    assert!(
        plan.typed_queries
            .iter()
            .any(|x| x.kind == "session_recent")
    );
    assert!(plan.typed_queries.iter().any(|x| x.kind == "session_om"));
    assert!(plan.notes.iter().any(|x| x == "session_om_hints:1"));
}

#[test]
fn search_query_plan_normalizes_mixed_case_om_hint_prefix() {
    let mut index = InMemoryIndex::new();
    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });
    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "oauth flow".to_string(),
        content: "oauth authorization code".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 1,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "oauth".to_string(),
            target_uri: None,
            session: Some("s-om-mixed".to_string()),
            session_hints: vec!["Om: compact long-term memory".to_string()],
            budget: None,
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "search".to_string(),
        },
    );

    let plan = result.query_plan;
    let session_om = plan
        .typed_queries
        .iter()
        .find(|typed| typed.kind == "session_om")
        .expect("session_om query");
    assert!(session_om.query.contains("compact long-term memory"));
    assert!(!session_om.query.contains("Om:"));
}

#[test]
fn drr_applies_filter_in_child_and_fallback_paths() {
    let mut index = InMemoryIndex::new();

    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });
    index.upsert(IndexRecord {
        id: "docs".to_string(),
        uri: "axiom://resources/docs".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "docs".to_string(),
        abstract_text: "docs".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 1,
    });
    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources/docs".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "auth".to_string(),
        content: "oauth".to_string(),
        tags: vec!["auth".to_string(), "markdown".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });
    index.upsert(IndexRecord {
        id: "storage".to_string(),
        uri: "axiom://resources/docs/storage.md".to_string(),
        parent_uri: Some("axiom://resources/docs".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "storage.md".to_string(),
        abstract_text: "storage".to_string(),
        content: "iops".to_string(),
        tags: vec!["storage".to_string(), "markdown".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "something-unseen".to_string(),
            target_uri: None,
            session: None,
            session_hints: Vec::new(),
            budget: None,
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: Some(SearchFilter {
                tags: vec!["auth".to_string()],
                mime: None,
            }),
            request_type: "find".to_string(),
        },
    );

    assert!(!result.query_results.is_empty());
    assert!(
        result
            .query_results
            .iter()
            .any(|x| x.uri == "axiom://resources/docs/auth.md")
    );
    assert!(
        !result
            .query_results
            .iter()
            .any(|x| x.uri == "axiom://resources/docs/storage.md")
    );
}

#[test]
fn drr_respects_target_uri_boundary_during_expansion_and_fallback() {
    let mut index = InMemoryIndex::new();
    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });
    index.upsert(IndexRecord {
        id: "src".to_string(),
        uri: "axiom://resources/mv-src".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "mv-src".to_string(),
        abstract_text: "source root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 1,
    });
    index.upsert(IndexRecord {
        id: "dst".to_string(),
        uri: "axiom://resources/mv-dst".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "mv-dst".to_string(),
        abstract_text: "destination root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 1,
    });
    index.upsert(IndexRecord {
        id: "src-guide".to_string(),
        uri: "axiom://resources/mv-src/guide.md".to_string(),
        parent_uri: Some("axiom://resources/mv-src".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "guide.md".to_string(),
        abstract_text: "moved token".to_string(),
        content: "moved_token in source".to_string(),
        tags: vec!["guide".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });
    index.upsert(IndexRecord {
        id: "dst-guide".to_string(),
        uri: "axiom://resources/mv-dst/guide.md".to_string(),
        parent_uri: Some("axiom://resources/mv-dst".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "guide.md".to_string(),
        abstract_text: "moved token".to_string(),
        content: "moved_token in destination".to_string(),
        tags: vec!["guide".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "moved_token".to_string(),
            target_uri: Some(
                AxiomUri::parse("axiom://resources/mv-src").expect("target uri parse"),
            ),
            session: None,
            session_hints: Vec::new(),
            budget: None,
            limit: 10,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "find".to_string(),
        },
    );

    assert!(!result.query_results.is_empty());
    assert!(
        result
            .query_results
            .iter()
            .all(|hit| hit.uri.starts_with("axiom://resources/mv-src"))
    );
}

#[test]
fn drr_respects_max_nodes_budget() {
    let mut index = InMemoryIndex::new();
    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });
    index.upsert(IndexRecord {
        id: "docs".to_string(),
        uri: "axiom://resources/docs".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "docs".to_string(),
        abstract_text: "docs".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 1,
    });
    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources/docs".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "auth".to_string(),
        content: "oauth".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "oauth".to_string(),
            target_uri: None,
            session: None,
            session_hints: Vec::new(),
            budget: Some(SearchBudget {
                max_ms: None,
                max_nodes: Some(1),
                max_depth: None,
            }),
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "find".to_string(),
        },
    );

    let trace = result.trace.expect("trace");
    assert!(trace.stop_reason.contains("budget_nodes"));
    assert!(trace.metrics.explored_nodes <= 1);
}

#[test]
fn drr_respects_max_depth_budget_including_fallback() {
    let mut index = InMemoryIndex::new();
    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });
    index.upsert(IndexRecord {
        id: "docs".to_string(),
        uri: "axiom://resources/docs".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "docs".to_string(),
        abstract_text: "docs".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 1,
    });
    index.upsert(IndexRecord {
        id: "auth".to_string(),
        uri: "axiom://resources/docs/auth.md".to_string(),
        parent_uri: Some("axiom://resources/docs".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "auth".to_string(),
        content: "oauth".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 2,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "unknown-query".to_string(),
            target_uri: None,
            session: None,
            session_hints: Vec::new(),
            budget: Some(SearchBudget {
                max_ms: None,
                max_nodes: None,
                max_depth: Some(1),
            }),
            limit: 10,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "find".to_string(),
        },
    );

    assert!(!result.query_results.is_empty());
    assert!(result.query_results.iter().all(|hit| {
        AxiomUri::parse(&hit.uri)
            .map(|uri| uri.segments().len() <= 1)
            .unwrap_or(false)
    }));
}

#[test]
fn drr_respects_max_ms_budget() {
    let mut index = InMemoryIndex::new();
    index.upsert(IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resource root".to_string(),
        content: String::new(),
        tags: vec![],
        updated_at: Utc::now(),
        depth: 0,
    });

    let engine = DrrEngine::new(DrrConfig::default());
    let result = engine.run(
        &index,
        &SearchOptions {
            query: "oauth".to_string(),
            target_uri: None,
            session: None,
            session_hints: Vec::new(),
            budget: Some(SearchBudget {
                max_ms: Some(0),
                max_nodes: None,
                max_depth: None,
            }),
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "find".to_string(),
        },
    );

    let trace = result.trace.expect("trace");
    assert!(trace.stop_reason.contains("budget_ms"));
    assert_eq!(trace.metrics.explored_nodes, 0);
}
