use std::collections::HashSet;

use crate::models::SearchOptions;
use crate::uri::{AxiomUri, Scope};

#[derive(Debug, Clone)]
pub(super) struct PlannedQuery {
    pub kind: String,
    pub query: String,
    pub scopes: Vec<Scope>,
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, Default)]
struct QueryIntent {
    wants_skill: bool,
    wants_memory: bool,
}

impl PlannedQuery {
    fn new(kind: &str, query: String, scopes: Vec<Scope>, priority: u8) -> Self {
        Self {
            kind: kind.to_string(),
            query,
            scopes: normalize_scopes(scopes),
            priority,
        }
    }
}

pub(super) fn plan_queries(options: &SearchOptions) -> Vec<PlannedQuery> {
    let intent = query_intent(&options.query);
    let base_scopes = intent_scopes(intent, options.target_uri.as_ref());
    let has_session_context = options.session.is_some();
    let mut planned = vec![PlannedQuery::new(
        "primary",
        options.query.clone(),
        base_scopes.clone(),
        1,
    )];

    if !options.request_type.starts_with("search") {
        return dedup_and_limit_queries(planned, 1);
    }

    if !options.session_hints.is_empty() {
        let hint_text = merge_non_om_hints(&options.session_hints);
        if !hint_text.is_empty() {
            let kind = if has_session_context {
                "session_recent"
            } else {
                "runtime_hints"
            };
            planned.push(PlannedQuery::new(
                kind,
                format!("{} {}", options.query, hint_text),
                base_scopes.clone(),
                2,
            ));
        }

        if has_session_context
            && let Some(om_hint) = options
                .session_hints
                .iter()
                .find(|hint| is_om_hint(hint))
                .and_then(|hint| normalize_om_hint(hint))
        {
            let om_scopes = if options.target_uri.is_some() {
                base_scopes
            } else {
                vec![Scope::User, Scope::Agent]
            };
            planned.push(PlannedQuery::new(
                "session_om",
                format!("{} {}", options.query, om_hint),
                om_scopes,
                2,
            ));
        }
    }

    if options.target_uri.is_none() {
        if intent.wants_skill {
            planned.push(PlannedQuery::new(
                "skill_focus",
                options.query.clone(),
                vec![Scope::Agent],
                2,
            ));
        }
        if intent.wants_memory || has_session_context {
            planned.push(PlannedQuery::new(
                "memory_focus",
                options.query.clone(),
                vec![Scope::User, Scope::Agent],
                3,
            ));
        }
    }

    dedup_and_limit_queries(planned, 5)
}

fn query_intent(query: &str) -> QueryIntent {
    let q = query.to_lowercase();
    QueryIntent {
        wants_skill: q.contains("skill"),
        wants_memory: q.contains("memory") || q.contains("preference") || q.contains("prefer"),
    }
}

fn intent_scopes(intent: QueryIntent, target: Option<&AxiomUri>) -> Vec<Scope> {
    if let Some(target) = target {
        return vec![target.scope()];
    }

    if intent.wants_skill {
        return vec![Scope::Agent];
    }
    if intent.wants_memory {
        return vec![Scope::User, Scope::Agent];
    }
    vec![Scope::Resources]
}

fn dedup_and_limit_queries(mut planned: Vec<PlannedQuery>, max_len: usize) -> Vec<PlannedQuery> {
    planned.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.query.cmp(&b.query))
    });

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in planned {
        let key = (item.query.to_lowercase(), item.scopes.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(item);
        if out.len() >= max_len {
            break;
        }
    }

    if out.is_empty() {
        out.push(PlannedQuery::new(
            "primary",
            String::new(),
            vec![Scope::Resources],
            1,
        ));
    }

    out
}

fn normalize_scopes(scopes: Vec<Scope>) -> Vec<Scope> {
    let mut scopes = scopes;
    scopes.sort_by_key(Scope::as_str);
    scopes.dedup();
    scopes
}

pub(super) fn is_om_hint(text: &str) -> bool {
    text.trim_start()
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("om:"))
}

fn merge_non_om_hints(hints: &[String]) -> String {
    let mut out = String::new();
    for hint in hints {
        if is_om_hint(hint) {
            continue;
        }
        let trimmed = hint.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(trimmed);
    }
    out
}

fn normalize_om_hint(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let without_prefix = match trimmed.split_once(':') {
        Some((prefix, rest)) if prefix.trim().eq_ignore_ascii_case("om") => rest.trim(),
        _ => trimmed,
    };
    if without_prefix.is_empty() {
        None
    } else {
        Some(without_prefix.to_string())
    }
}

pub(super) fn collect_scope_names(planned_queries: &[PlannedQuery]) -> Vec<String> {
    let mut scopes = planned_queries
        .iter()
        .flat_map(|x| x.scopes.iter().copied())
        .collect::<Vec<_>>();
    scopes.sort_by_key(Scope::as_str);
    scopes.dedup();
    scopes
        .into_iter()
        .map(|scope| scope.as_str().to_string())
        .collect()
}

pub(super) fn uri_in_scopes(uri: &str, scopes: &[Scope]) -> bool {
    if scopes.is_empty() {
        return true;
    }
    let Ok(parsed) = AxiomUri::parse(uri) else {
        return false;
    };
    scopes.iter().any(|scope| parsed.scope() == *scope)
}

#[cfg(test)]
mod tests {
    use super::{
        PlannedQuery, collect_scope_names, dedup_and_limit_queries, is_om_hint, merge_non_om_hints,
        normalize_scopes, query_intent,
    };
    use crate::uri::Scope;

    #[test]
    fn normalize_scopes_is_value_based_and_sorted() {
        let scopes = normalize_scopes(vec![
            Scope::Resources,
            Scope::User,
            Scope::Resources,
            Scope::Agent,
        ]);
        assert_eq!(scopes, vec![Scope::Agent, Scope::Resources, Scope::User]);
    }

    #[test]
    fn dedup_queries_ignores_scope_order_after_normalization() {
        let queries = vec![
            PlannedQuery::new(
                "primary",
                "oauth flow".to_string(),
                vec![Scope::User, Scope::Resources],
                1,
            ),
            PlannedQuery::new(
                "primary",
                "OAUTH FLOW".to_string(),
                vec![Scope::Resources, Scope::User],
                1,
            ),
        ];
        let deduped = dedup_and_limit_queries(queries, 5);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn collect_scope_names_returns_sorted_distinct_names() {
        let planned = vec![
            PlannedQuery::new(
                "primary",
                "q".to_string(),
                vec![Scope::Resources, Scope::User],
                1,
            ),
            PlannedQuery::new(
                "secondary",
                "q2".to_string(),
                vec![Scope::Agent, Scope::User],
                2,
            ),
        ];
        let names = collect_scope_names(&planned);
        assert_eq!(
            names,
            vec![
                "agent".to_string(),
                "resources".to_string(),
                "user".to_string()
            ]
        );
    }

    #[test]
    fn query_intent_parses_skill_and_memory_flags() {
        let skill = query_intent("show skill docs");
        assert!(skill.wants_skill);
        assert!(!skill.wants_memory);

        let memory = query_intent("memory preference profile");
        assert!(!memory.wants_skill);
        assert!(memory.wants_memory);
    }

    #[test]
    fn is_om_hint_is_case_insensitive_and_trim_aware() {
        assert!(is_om_hint("om: hint"));
        assert!(is_om_hint("  Om: hint"));
        assert!(is_om_hint("\tOM: hint"));
        assert!(!is_om_hint("hint om: value"));
        assert!(!is_om_hint("memo: hint"));
    }

    #[test]
    fn merge_non_om_hints_skips_om_prefixed_entries() {
        let hints = vec![
            "recent one".to_string(),
            "  om: long memory".to_string(),
            " recent two ".to_string(),
            " ".to_string(),
        ];
        assert_eq!(merge_non_om_hints(&hints), "recent one recent two");
    }
}
