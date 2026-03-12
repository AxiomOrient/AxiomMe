use std::collections::HashMap;
use std::time::Instant;

use uuid::Uuid;

use crate::index::InMemoryIndex;
use crate::models::{
    ContextHit, FindResult, QueryPlan, RetrievalStep, RetrievalTrace, SearchOptions, TracePoint,
    TraceStats, classify_hit_buckets,
};

use super::budget::{ResolvedBudget, resolve_budget};
use super::config::DrrConfig;
use super::expansion::run_single_query;
use super::planner::{PlannedQuery, collect_scope_names, is_om_hint, plan_queries};
use super::scoring::{
    fanout_priority_weight, merge_hits, merge_trace_points, scale_hit_scores,
    scale_trace_point_scores, sort_hits_by_score_desc_uri_asc, sorted_trace_points,
    tokenize_keywords, typed_query_plans,
};

#[derive(Debug, Clone)]
pub struct DrrEngine {
    config: DrrConfig,
}

const FANOUT_PRIORITY_WEIGHT_NOTE: &str = "fanout_weight:p1=1.00,p2=0.82,p3=0.64,p4+=0.46";

impl DrrEngine {
    #[must_use]
    pub const fn new(config: DrrConfig) -> Self {
        Self { config }
    }

    pub fn run(&self, index: &InMemoryIndex, options: &SearchOptions) -> FindResult {
        let start = Instant::now();
        let trace_id = Uuid::new_v4().to_string();
        let planned_queries = plan_queries(options);
        let request_budget = resolve_budget(&self.config, options.budget.as_ref());
        let fanout = execute_planned_queries(
            &self.config,
            index,
            options,
            &planned_queries,
            request_budget,
            start,
        );

        let limit = options.limit.max(1);
        let mut hits: Vec<_> = fanout.merged_hits.into_values().collect();
        sort_hits_by_score_desc_uri_asc(&mut hits);
        hits.truncate(limit);

        let final_topk = hits
            .iter()
            .map(|h| TracePoint {
                uri: h.uri.clone(),
                score: h.score,
            })
            .collect::<Vec<_>>();

        let start_points = sorted_trace_points(fanout.merged_start_points);
        let stop_reason = build_stop_reason(&fanout.stop_reasons);

        let trace = RetrievalTrace {
            trace_id,
            request_type: options.request_type.clone(),
            query: options.query.clone(),
            target_uri: options.target_uri.as_ref().map(ToString::to_string),
            start_points,
            steps: fanout.merged_steps,
            final_topk,
            stop_reason,
            metrics: TraceStats {
                latency_ms: start.elapsed().as_millis(),
                explored_nodes: fanout.explored_nodes,
                convergence_rounds: fanout.convergence_rounds,
                typed_query_count: planned_queries.len(),
                relation_enriched_hits: 0,
                relation_enriched_links: 0,
            },
        };

        let hit_buckets = classify_hit_buckets(&hits);
        let notes = build_query_notes(options, request_budget, planned_queries.len());
        let memories = hit_buckets
            .memories
            .iter()
            .filter_map(|&index| hits.get(index).cloned())
            .collect::<Vec<_>>();
        let resources = hit_buckets
            .resources
            .iter()
            .filter_map(|&index| hits.get(index).cloned())
            .collect::<Vec<_>>();
        let skills = hit_buckets
            .skills
            .iter()
            .filter_map(|&index| hits.get(index).cloned())
            .collect::<Vec<_>>();

        FindResult {
            query_plan: QueryPlan {
                scopes: collect_scope_names(&planned_queries),
                keywords: tokenize_keywords(&options.query),
                typed_queries: typed_query_plans(&planned_queries),
                notes,
            },
            query_results: hits,
            hit_buckets,
            memories,
            resources,
            skills,
            trace: Some(trace),
            trace_uri: None,
        }
    }
}

#[derive(Debug, Default)]
struct FanoutState {
    merged_hits: HashMap<String, ContextHit>,
    merged_start_points: HashMap<String, f32>,
    merged_steps: Vec<RetrievalStep>,
    explored_nodes: usize,
    convergence_rounds: u32,
    stop_reasons: Vec<String>,
}

fn execute_planned_queries(
    config: &DrrConfig,
    index: &InMemoryIndex,
    options: &SearchOptions,
    planned_queries: &[PlannedQuery],
    request_budget: ResolvedBudget,
    start: Instant,
) -> FanoutState {
    let mut state = FanoutState::default();
    let mut round_offset = 0u32;
    let mut remaining_nodes = request_budget.nodes;

    for planned in planned_queries {
        if remaining_nodes == 0 {
            state.stop_reasons.push("budget_nodes".to_string());
            break;
        }

        let remaining_ms = request_budget.time_ms.map(|max_ms| {
            let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            max_ms.saturating_sub(elapsed)
        });
        if remaining_ms == Some(0) {
            state.stop_reasons.push("budget_ms".to_string());
            break;
        }

        let mut single = run_single_query(
            config,
            index,
            options,
            planned,
            ResolvedBudget {
                time_ms: remaining_ms,
                nodes: remaining_nodes,
                depth: request_budget.depth,
            },
        );
        let weight = fanout_priority_weight(planned.priority);
        scale_hit_scores(&mut single.hits, weight);
        scale_trace_point_scores(&mut single.trace.start_points, weight);
        merge_hits(&mut state.merged_hits, single.hits);
        merge_trace_points(&mut state.merged_start_points, &single.trace.start_points);

        for step in single.trace.steps {
            state.merged_steps.push(RetrievalStep {
                round: step.round.saturating_add(round_offset),
                current_uri: step.current_uri,
                children_examined: step.children_examined,
                children_selected: step.children_selected,
                queue_size_after: step.queue_size_after,
            });
        }
        if let Some(last_round) = state.merged_steps.last().map(|step| step.round) {
            round_offset = last_round;
        }

        state.explored_nodes += single.trace.metrics.explored_nodes;
        state.convergence_rounds += single.trace.metrics.convergence_rounds;
        state.stop_reasons.push(single.trace.stop_reason);
        remaining_nodes = remaining_nodes.saturating_sub(single.trace.metrics.explored_nodes);
    }

    state
}

fn build_stop_reason(stop_reasons: &[String]) -> String {
    if stop_reasons.len() <= 1 {
        stop_reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "queue_empty".to_string())
    } else {
        format!("fanout:{}", stop_reasons.join("|"))
    }
}

fn build_query_notes(
    options: &SearchOptions,
    request_budget: ResolvedBudget,
    planned_query_count: usize,
) -> Vec<String> {
    let mut notes = vec![
        "drr".to_string(),
        format!("fanout:{planned_query_count}"),
        FANOUT_PRIORITY_WEIGHT_NOTE.to_string(),
        format!("budget_nodes:{}", request_budget.nodes),
        format!("budget_depth:{}", request_budget.depth),
    ];
    if !options.session_hints.is_empty() {
        notes.push(format!("session_hints:{}", options.session_hints.len()));
        let om_hint_count = options
            .session_hints
            .iter()
            .filter(|hint| is_om_hint(hint))
            .count();
        notes.push(format!("session_om_hints:{om_hint_count}"));
    }
    if options.filter.is_some() {
        notes.push("filter".to_string());
    }
    if let Some(max_ms) = request_budget.time_ms {
        notes.push(format!("budget_ms:{max_ms}"));
    }
    notes
}
