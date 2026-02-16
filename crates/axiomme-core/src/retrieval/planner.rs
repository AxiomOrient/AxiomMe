use std::collections::{HashMap, HashSet};

use crate::models::SearchOptions;
use crate::uri::{AxiomUri, Scope};

#[derive(Debug, Clone)]
pub(super) struct PlannedQuery {
    pub kind: String,
    pub query: String,
    pub scopes: Vec<Scope>,
    pub priority: u8,
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

pub(super) trait IntentPlanner {
    fn plan(&self, options: &SearchOptions) -> Vec<PlannedQuery>;
}

#[derive(Debug, Clone, Default)]
pub(super) struct RuleIntentPlanner;

impl IntentPlanner for RuleIntentPlanner {
    fn plan(&self, options: &SearchOptions) -> Vec<PlannedQuery> {
        let base_scopes = intent_scopes(&options.query, options.target_uri.as_ref());
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
            let hint_text = options
                .session_hints
                .iter()
                .filter(|hint| !is_om_hint(hint))
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if !hint_text.trim().is_empty() {
                planned.push(PlannedQuery::new(
                    "session_recent",
                    format!("{} {}", options.query, hint_text),
                    base_scopes.clone(),
                    2,
                ));
            }

            if let Some(om_hint) = options
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
            let query_lower = options.query.to_lowercase();
            if query_lower.contains("skill") {
                planned.push(PlannedQuery::new(
                    "skill_focus",
                    options.query.clone(),
                    vec![Scope::Agent],
                    2,
                ));
            }
            if query_lower.contains("memory")
                || query_lower.contains("preference")
                || query_lower.contains("prefer")
                || !options.session_hints.is_empty()
            {
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
}

fn intent_scopes(query: &str, target: Option<&AxiomUri>) -> Vec<Scope> {
    if let Some(target) = target {
        return vec![target.scope()];
    }

    let q = query.to_lowercase();
    if q.contains("skill") {
        return vec![Scope::Agent];
    }
    if q.contains("memory") || q.contains("preference") || q.contains("prefer") {
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
        let key = format!(
            "{}|{}",
            item.query.to_lowercase(),
            item.scopes
                .iter()
                .map(Scope::as_str)
                .collect::<Vec<_>>()
                .join(",")
        );
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
    let mut map = HashMap::<String, Scope>::new();
    for scope in scopes {
        map.insert(scope.as_str().to_string(), scope);
    }

    let mut names = map.keys().cloned().collect::<Vec<_>>();
    names.sort();

    names
        .into_iter()
        .filter_map(|name| map.remove(&name))
        .collect()
}

fn is_om_hint(text: &str) -> bool {
    text.trim_start().to_ascii_lowercase().starts_with("om:")
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
    let mut names = planned_queries
        .iter()
        .flat_map(|x| x.scopes.iter().map(|s| s.as_str().to_string()))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
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
