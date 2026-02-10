use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::time::Instant;

use uuid::Uuid;

use crate::index::InMemoryHybridIndex;
use crate::models::{
    ContextHit, FindResult, QueryPlan, RetrievalStep, RetrievalTrace, SearchBudget, SearchOptions,
    TracePoint, TraceStats, TypedQueryPlan,
};
use crate::uri::{AxiomUri, Scope};

#[derive(Debug, Clone)]
pub struct DrrConfig {
    pub alpha: f32,
    pub global_topk: usize,
    pub max_convergence_rounds: u32,
    pub max_depth: usize,
    pub max_nodes: usize,
}

impl Default for DrrConfig {
    fn default() -> Self {
        Self {
            alpha: 0.5,
            global_topk: 3,
            max_convergence_rounds: 3,
            max_depth: 5,
            max_nodes: 256,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrrEngine {
    config: DrrConfig,
    planner: RuleIntentPlanner,
}

impl DrrEngine {
    pub fn new(config: DrrConfig) -> Self {
        Self {
            config,
            planner: RuleIntentPlanner,
        }
    }

    pub fn run(&self, index: &InMemoryHybridIndex, options: SearchOptions) -> FindResult {
        let start = Instant::now();
        let trace_id = Uuid::new_v4().to_string();
        let planned_queries = self.planner.plan(&options);
        let request_budget = resolve_budget(&self.config, options.budget.as_ref());

        let mut merged_hits = HashMap::<String, ContextHit>::new();
        let mut merged_start_points = HashMap::<String, f32>::new();
        let mut merged_steps = Vec::<RetrievalStep>::new();
        let mut round_offset = 0u32;
        let mut explored_nodes = 0usize;
        let mut convergence_rounds = 0u32;
        let mut stop_reasons = Vec::<String>::new();
        let mut remaining_nodes = request_budget.max_nodes;

        for planned in &planned_queries {
            if remaining_nodes == 0 {
                stop_reasons.push("budget_nodes".to_string());
                break;
            }

            let remaining_ms = request_budget.max_ms.map(|max_ms| {
                let elapsed = start.elapsed().as_millis() as u64;
                max_ms.saturating_sub(elapsed)
            });
            if let Some(0) = remaining_ms {
                stop_reasons.push("budget_ms".to_string());
                break;
            }

            let single = self.run_single(
                index,
                &options,
                planned,
                ResolvedBudget {
                    max_ms: remaining_ms,
                    max_nodes: remaining_nodes,
                    max_depth: request_budget.max_depth,
                },
            );
            merge_hits(&mut merged_hits, single.hits);
            merge_trace_points(&mut merged_start_points, &single.trace.start_points);

            for step in single.trace.steps {
                merged_steps.push(RetrievalStep {
                    round: step.round.saturating_add(round_offset),
                    current_uri: step.current_uri,
                    children_examined: step.children_examined,
                    children_selected: step.children_selected,
                    queue_size_after: step.queue_size_after,
                });
            }

            if let Some(last_round) = merged_steps.last().map(|x| x.round) {
                round_offset = last_round;
            }

            explored_nodes += single.trace.metrics.explored_nodes;
            convergence_rounds += single.trace.metrics.convergence_rounds;
            stop_reasons.push(single.trace.stop_reason);
            remaining_nodes = remaining_nodes.saturating_sub(single.trace.metrics.explored_nodes);
        }

        let limit = options.limit.max(1);
        let mut hits: Vec<_> = merged_hits.into_values().collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        hits.truncate(limit);

        let final_topk = hits
            .iter()
            .map(|h| TracePoint {
                uri: h.uri.clone(),
                score: h.score,
            })
            .collect::<Vec<_>>();

        let start_points = sorted_trace_points(merged_start_points);
        let stop_reason = if stop_reasons.len() <= 1 {
            stop_reasons
                .first()
                .cloned()
                .unwrap_or_else(|| "queue_empty".to_string())
        } else {
            format!("fanout:{}", stop_reasons.join("|"))
        };

        let trace = RetrievalTrace {
            trace_id,
            request_type: options.request_type.clone(),
            query: options.query.clone(),
            target_uri: options.target_uri.as_ref().map(ToString::to_string),
            start_points,
            steps: merged_steps,
            final_topk,
            stop_reason,
            metrics: TraceStats {
                latency_ms: start.elapsed().as_millis(),
                explored_nodes,
                convergence_rounds,
                typed_query_count: planned_queries.len(),
                relation_enriched_hits: 0,
                relation_enriched_links: 0,
            },
        };

        let (memories, resources, skills) = split_hits(&hits);
        let mut notes = vec![
            "drr".to_string(),
            format!("fanout:{}", planned_queries.len()),
            format!("budget_nodes:{}", request_budget.max_nodes),
            format!("budget_depth:{}", request_budget.max_depth),
        ];
        if options.filter.is_some() {
            notes.push("filter".to_string());
        }
        if let Some(max_ms) = request_budget.max_ms {
            notes.push(format!("budget_ms:{}", max_ms));
        }

        FindResult {
            memories,
            resources,
            skills,
            query_plan: serde_json::to_value(QueryPlan {
                scopes: collect_scope_names(&planned_queries),
                keywords: tokenize_keywords(&options.query),
                typed_queries: planned_queries
                    .iter()
                    .map(|x| TypedQueryPlan {
                        kind: x.kind.clone(),
                        query: x.query.clone(),
                        scopes: x.scopes.iter().map(|s| s.as_str().to_string()).collect(),
                        priority: x.priority,
                    })
                    .collect(),
                notes,
            })
            .unwrap_or(serde_json::json!({})),
            query_results: hits,
            trace: Some(trace),
            trace_uri: None,
        }
    }

    fn run_single(
        &self,
        index: &InMemoryHybridIndex,
        options: &SearchOptions,
        planned: &PlannedQuery,
        budget: ResolvedBudget,
    ) -> SingleRunResult {
        let run_start = Instant::now();
        let trace_id = Uuid::new_v4().to_string();
        let query = planned.query.clone();
        let limit = options.limit.max(1);
        let target = options.target_uri.clone();
        let filter = options.filter.as_ref();

        let root_records = index
            .scope_roots(&planned.scopes)
            .into_iter()
            .filter(|record| {
                record.depth <= budget.max_depth && index.record_matches_filter(record, filter)
            })
            .collect::<Vec<_>>();
        let mut global_dirs =
            index.search_directories(&query, target.as_ref(), self.config.global_topk, filter);
        global_dirs.retain(|x| {
            uri_in_scopes(&x.record.uri, &planned.scopes) && x.record.depth <= budget.max_depth
        });

        let mut global_rank = index.search(
            &query,
            target.as_ref(),
            limit.max(32),
            options.score_threshold,
            filter,
        );
        global_rank.retain(|x| {
            uri_in_scopes(&x.record.uri, &planned.scopes) && x.record.depth <= budget.max_depth
        });

        let score_map = global_rank
            .iter()
            .map(|s| (s.record.uri.clone(), s.score))
            .collect::<HashMap<_, _>>();

        let mut trace_start = Vec::new();
        let mut frontier = BinaryHeap::new();
        let mut seen_start = HashSet::new();

        for root in &root_records {
            if seen_start.insert(root.uri.clone()) {
                trace_start.push(TracePoint {
                    uri: root.uri.clone(),
                    score: 0.0,
                });
                frontier.push(Node {
                    uri: root.uri.clone(),
                    score: 0.0,
                    depth: root.depth,
                });
            }
        }

        for dir in &global_dirs {
            if seen_start.insert(dir.record.uri.clone()) {
                trace_start.push(TracePoint {
                    uri: dir.record.uri.clone(),
                    score: dir.score,
                });
                frontier.push(Node {
                    uri: dir.record.uri.clone(),
                    score: dir.score,
                    depth: dir.record.depth,
                });
            }
        }

        let mut steps = Vec::new();
        let mut visited = HashSet::new();
        let mut explored = 0usize;
        let mut round = 0u32;
        let mut stable_rounds = 0u32;
        let mut previous_topk = Vec::<String>::new();
        let mut selected = HashMap::<String, ContextHit>::new();
        let mut stop_reason = "queue_empty".to_string();

        while let Some(node) = frontier.pop() {
            if let Some(max_ms) = budget.max_ms
                && run_start.elapsed().as_millis() >= max_ms as u128
            {
                stop_reason = "budget_ms".to_string();
                break;
            }
            if explored >= budget.max_nodes {
                stop_reason = "budget_nodes".to_string();
                break;
            }
            if node.depth > budget.max_depth {
                stop_reason = "max_depth".to_string();
                continue;
            }
            if !visited.insert(node.uri.clone()) {
                continue;
            }

            round += 1;
            explored += 1;

            let children = index.children_of(&node.uri);
            let children_examined = children.len();
            let mut children_selected = 0usize;

            for child in children {
                if !uri_in_scopes(&child.uri, &planned.scopes) {
                    continue;
                }
                if child.depth > budget.max_depth {
                    continue;
                }
                if !index.record_matches_filter(&child, filter) {
                    continue;
                }

                let local_score = *score_map.get(&child.uri).unwrap_or(&0.0);
                let propagated =
                    self.config.alpha * local_score + (1.0 - self.config.alpha) * node.score;

                if child.is_leaf {
                    let hit = make_hit(&child, propagated);
                    selected
                        .entry(hit.uri.clone())
                        .and_modify(|existing| {
                            if propagated > existing.score {
                                *existing = hit.clone();
                            }
                        })
                        .or_insert(hit);
                    children_selected += 1;
                } else if child.depth <= budget.max_depth {
                    frontier.push(Node {
                        uri: child.uri,
                        score: propagated,
                        depth: child.depth,
                    });
                    children_selected += 1;
                }
            }

            steps.push(RetrievalStep {
                round,
                current_uri: node.uri,
                children_examined,
                children_selected,
                queue_size_after: frontier.len(),
            });

            let mut candidate = selected.values().cloned().collect::<Vec<_>>();
            candidate.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
            let topk = candidate
                .iter()
                .take(limit)
                .map(|h| h.uri.clone())
                .collect::<Vec<_>>();

            if topk == previous_topk {
                stable_rounds += 1;
                if stable_rounds >= self.config.max_convergence_rounds {
                    stop_reason = "converged".to_string();
                    break;
                }
            } else {
                stable_rounds = 0;
            }
            previous_topk = topk;
        }

        if selected.is_empty() {
            for scored in global_rank.iter().take(limit) {
                selected.insert(
                    scored.record.uri.clone(),
                    make_hit(&scored.record, scored.score),
                );
            }
        }

        let mut hits: Vec<_> = selected.into_values().collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        hits.truncate(limit);

        let final_topk = hits
            .iter()
            .map(|h| TracePoint {
                uri: h.uri.clone(),
                score: h.score,
            })
            .collect::<Vec<_>>();

        let trace = RetrievalTrace {
            trace_id,
            request_type: options.request_type.clone(),
            query,
            target_uri: target.as_ref().map(ToString::to_string),
            start_points: trace_start,
            steps,
            final_topk,
            stop_reason,
            metrics: TraceStats {
                latency_ms: run_start.elapsed().as_millis(),
                explored_nodes: explored,
                convergence_rounds: stable_rounds,
                typed_query_count: 1,
                relation_enriched_hits: 0,
                relation_enriched_links: 0,
            },
        };

        SingleRunResult { hits, trace }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResolvedBudget {
    max_ms: Option<u64>,
    max_nodes: usize,
    max_depth: usize,
}

fn resolve_budget(config: &DrrConfig, budget: Option<&SearchBudget>) -> ResolvedBudget {
    let max_nodes = budget
        .and_then(|x| x.max_nodes)
        .unwrap_or(config.max_nodes)
        .max(1);
    let max_depth = budget
        .and_then(|x| x.max_depth)
        .unwrap_or(config.max_depth)
        .max(1);
    let max_ms = budget.and_then(|x| x.max_ms);
    ResolvedBudget {
        max_ms,
        max_nodes,
        max_depth,
    }
}

#[derive(Debug, Clone)]
struct Node {
    uri: String,
    score: f32,
    depth: usize,
}

impl Eq for Node {}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri && self.score == other.score
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.uri.cmp(&other.uri))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
struct PlannedQuery {
    kind: String,
    query: String,
    scopes: Vec<Scope>,
    priority: u8,
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

trait IntentPlanner {
    fn plan(&self, options: &SearchOptions) -> Vec<PlannedQuery>;
}

#[derive(Debug, Clone, Default)]
struct RuleIntentPlanner;

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
                .take(2)
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

#[derive(Debug, Clone)]
struct SingleRunResult {
    hits: Vec<ContextHit>,
    trace: RetrievalTrace,
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

fn split_hits(hits: &[ContextHit]) -> (Vec<ContextHit>, Vec<ContextHit>, Vec<ContextHit>) {
    let mut memories = Vec::new();
    let mut resources = Vec::new();
    let mut skills = Vec::new();

    for hit in hits {
        if hit.uri.starts_with("axiom://user/memories")
            || hit.uri.starts_with("axiom://agent/memories")
        {
            memories.push(hit.clone());
        } else if hit.uri.starts_with("axiom://agent/skills") {
            skills.push(hit.clone());
        } else {
            resources.push(hit.clone());
        }
    }

    (memories, resources, skills)
}

fn make_hit(record: &crate::models::IndexRecord, score: f32) -> ContextHit {
    ContextHit {
        uri: record.uri.clone(),
        score,
        abstract_text: record.abstract_text.clone(),
        context_type: record.context_type.clone(),
        relations: Vec::new(),
    }
}

fn tokenize_keywords(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| !x.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn merge_hits(acc: &mut HashMap<String, ContextHit>, hits: Vec<ContextHit>) {
    for hit in hits {
        acc.entry(hit.uri.clone())
            .and_modify(|existing| {
                if hit.score > existing.score {
                    *existing = hit.clone();
                }
            })
            .or_insert(hit);
    }
}

fn merge_trace_points(acc: &mut HashMap<String, f32>, points: &[TracePoint]) {
    for point in points {
        acc.entry(point.uri.clone())
            .and_modify(|score| {
                if point.score > *score {
                    *score = point.score;
                }
            })
            .or_insert(point.score);
    }
}

fn sorted_trace_points(points: HashMap<String, f32>) -> Vec<TracePoint> {
    let mut out = points
        .into_iter()
        .map(|(uri, score)| TracePoint { uri, score })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.uri.cmp(&b.uri))
    });
    out
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
                .map(|x| x.as_str())
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

fn collect_scope_names(planned_queries: &[PlannedQuery]) -> Vec<String> {
    let mut names = planned_queries
        .iter()
        .flat_map(|x| x.scopes.iter().map(|s| s.as_str().to_string()))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn uri_in_scopes(uri: &str, scopes: &[Scope]) -> bool {
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
    use chrono::Utc;

    use super::*;
    use crate::index::InMemoryHybridIndex;
    use crate::models::{IndexRecord, QueryPlan, SearchBudget, SearchFilter, SearchOptions};
    use crate::uri::AxiomUri;

    #[test]
    fn drr_returns_trace_and_hits() {
        let mut index = InMemoryHybridIndex::new();

        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "root".to_string(),
            content: "".to_string(),
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
            SearchOptions {
                query: "oauth".to_string(),
                target_uri: None,
                session: None,
                session_hints: Vec::new(),
                budget: None,
                limit: 5,
                score_threshold: None,
                filter: None,
                request_type: "find".to_string(),
            },
        );

        assert!(!result.query_results.is_empty());
        assert!(result.trace.is_some());
    }

    #[test]
    fn search_query_plan_includes_typed_queries() {
        let mut index = InMemoryHybridIndex::new();

        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "resource root".to_string(),
            content: "".to_string(),
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
            SearchOptions {
                query: "oauth".to_string(),
                target_uri: None,
                session: Some("s-1".to_string()),
                session_hints: vec!["use refresh token".to_string()],
                budget: None,
                limit: 5,
                score_threshold: None,
                filter: None,
                request_type: "search".to_string(),
            },
        );

        let plan: QueryPlan = serde_json::from_value(result.query_plan).expect("query plan");
        assert!(plan.typed_queries.iter().any(|x| x.kind == "primary"));
        assert!(
            plan.typed_queries
                .iter()
                .any(|x| x.kind == "session_recent")
        );
        assert!(plan.typed_queries.len() >= 2);
    }

    #[test]
    fn drr_applies_filter_in_child_and_fallback_paths() {
        let mut index = InMemoryHybridIndex::new();

        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "resource root".to_string(),
            content: "".to_string(),
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
            content: "".to_string(),
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
            SearchOptions {
                query: "something-unseen".to_string(),
                target_uri: None,
                session: None,
                session_hints: Vec::new(),
                budget: None,
                limit: 5,
                score_threshold: None,
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
    fn drr_respects_max_nodes_budget() {
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "resource root".to_string(),
            content: "".to_string(),
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
            content: "".to_string(),
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
            SearchOptions {
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
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "resource root".to_string(),
            content: "".to_string(),
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
            content: "".to_string(),
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
            SearchOptions {
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
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "resource root".to_string(),
            content: "".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 0,
        });

        let engine = DrrEngine::new(DrrConfig::default());
        let result = engine.run(
            &index,
            SearchOptions {
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
                filter: None,
                request_type: "find".to_string(),
            },
        );

        let trace = result.trace.expect("trace");
        assert!(trace.stop_reason.contains("budget_ms"));
        assert_eq!(trace.metrics.explored_nodes, 0);
    }
}
