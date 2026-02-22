use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::time::Instant;

use uuid::Uuid;

use crate::index::{InMemoryIndex, ScoredRecord};
use crate::models::{
    ContextHit, RetrievalStep, RetrievalTrace, SearchOptions, TracePoint, TraceStats,
};
use crate::uri::AxiomUri;

use super::budget::ResolvedBudget;
use super::config::DrrConfig;
use super::planner::{PlannedQuery, uri_in_scopes};
use super::scoring::make_hit;

#[derive(Debug, Clone)]
pub(super) struct SingleRunResult {
    pub hits: Vec<ContextHit>,
    pub trace: RetrievalTrace,
}

struct QueryInitialization {
    trace_start: Vec<TracePoint>,
    frontier: BinaryHeap<Node>,
    score_map: HashMap<String, f32>,
    global_rank: Vec<ScoredRecord>,
    filter_projection: Option<HashSet<String>>,
}

struct FinalizeSingleQueryInput<'a> {
    selected: HashMap<String, ContextHit>,
    global_rank: &'a [ScoredRecord],
    limit: usize,
    trace_id: String,
    options: &'a SearchOptions,
    query: String,
    target: Option<crate::uri::AxiomUri>,
    trace_start: Vec<TracePoint>,
    steps: Vec<RetrievalStep>,
    stop_reason: String,
    explored: usize,
    stable_rounds: u32,
    latency_ms: u128,
}

struct ExpansionLoopState {
    steps: Vec<RetrievalStep>,
    selected: HashMap<String, ContextHit>,
    explored: usize,
    stable_rounds: u32,
    stop_reason: String,
}

struct ExpansionLoopInput<'a> {
    config: &'a DrrConfig,
    index: &'a InMemoryIndex,
    planned: &'a PlannedQuery,
    budget: ResolvedBudget,
    target: Option<&'a AxiomUri>,
    filter_projection: Option<&'a HashSet<String>>,
    limit: usize,
    score_map: &'a HashMap<String, f32>,
    frontier: BinaryHeap<Node>,
    run_start: Instant,
}

pub(super) fn run_single_query(
    config: &DrrConfig,
    index: &InMemoryIndex,
    options: &SearchOptions,
    planned: &PlannedQuery,
    budget: ResolvedBudget,
) -> SingleRunResult {
    let run_start = Instant::now();
    let trace_id = Uuid::new_v4().to_string();
    let query = planned.query.clone();
    let limit = options.limit.max(1);
    let target = options.target_uri.clone();
    let QueryInitialization {
        trace_start,
        frontier,
        score_map,
        global_rank,
        filter_projection,
    } = initialize_query_frontier(config, index, options, planned, budget, &query, limit);
    let loop_state = execute_expansion_loop(ExpansionLoopInput {
        config,
        index,
        planned,
        budget,
        target: target.as_ref(),
        filter_projection: filter_projection.as_ref(),
        limit,
        score_map: &score_map,
        frontier,
        run_start,
    });

    finalize_single_query_run(FinalizeSingleQueryInput {
        selected: loop_state.selected,
        global_rank: &global_rank,
        limit,
        trace_id,
        options,
        query,
        target,
        trace_start,
        steps: loop_state.steps,
        stop_reason: loop_state.stop_reason,
        explored: loop_state.explored,
        stable_rounds: loop_state.stable_rounds,
        latency_ms: run_start.elapsed().as_millis(),
    })
}

fn execute_expansion_loop(input: ExpansionLoopInput<'_>) -> ExpansionLoopState {
    let ExpansionLoopInput {
        config,
        index,
        planned,
        budget,
        target,
        filter_projection,
        limit,
        score_map,
        mut frontier,
        run_start,
    } = input;
    let mut steps = Vec::new();
    let mut visited = HashSet::new();
    let mut explored = 0usize;
    let mut round = 0u32;
    let mut stable_rounds = 0u32;
    let mut previous_topk = Vec::<String>::new();
    let mut selected = HashMap::<String, ContextHit>::new();
    let mut stop_reason = "queue_empty".to_string();

    while let Some(node) = frontier.pop() {
        if let Some(max_ms) = budget.time_ms
            && run_start.elapsed().as_millis() >= u128::from(max_ms)
        {
            stop_reason = "budget_ms".to_string();
            break;
        }
        if explored >= budget.nodes {
            stop_reason = "budget_nodes".to_string();
            break;
        }
        if node.depth > budget.depth {
            stop_reason = "max_depth".to_string();
            continue;
        }
        if !visited.insert(node.uri.clone()) {
            continue;
        }

        round = round.saturating_add(1);
        explored = explored.saturating_add(1);

        let children = index.children_of(&node.uri);
        let children_examined = children.len();
        let mut children_selected = 0usize;

        for child in children {
            if !uri_matches_query_bounds(&child.uri, planned, target)
                || child.depth > budget.depth
                || !uri_matches_filter_projection(&child.uri, filter_projection)
            {
                continue;
            }
            let local_score = *score_map.get(&child.uri).unwrap_or(&0.0);
            let propagated = local_score.mul_add(config.alpha, (1.0 - config.alpha) * node.score);
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
                children_selected = children_selected.saturating_add(1);
                continue;
            }
            frontier.push(Node {
                uri: child.uri,
                score: propagated,
                depth: child.depth,
            });
            children_selected = children_selected.saturating_add(1);
        }

        steps.push(RetrievalStep {
            round,
            current_uri: node.uri,
            children_examined,
            children_selected,
            queue_size_after: frontier.len(),
        });

        if update_convergence_state(
            &selected,
            limit,
            &mut previous_topk,
            &mut stable_rounds,
            config.max_convergence_rounds,
        ) {
            stop_reason = "converged".to_string();
            break;
        }
    }

    ExpansionLoopState {
        steps,
        selected,
        explored,
        stable_rounds,
        stop_reason,
    }
}

fn uri_matches_query_bounds(uri: &str, planned: &PlannedQuery, target: Option<&AxiomUri>) -> bool {
    if !uri_in_scopes(uri, &planned.scopes) {
        return false;
    }
    uri_in_target(uri, target)
}

fn uri_matches_filter_projection(uri: &str, filter_projection: Option<&HashSet<String>>) -> bool {
    match filter_projection {
        Some(allowed_uris) => allowed_uris.contains(uri),
        None => true,
    }
}

fn uri_in_target(uri: &str, target: Option<&AxiomUri>) -> bool {
    let Some(target) = target else {
        return true;
    };
    let Ok(parsed) = AxiomUri::parse(uri) else {
        return false;
    };
    parsed.starts_with(target)
}

fn update_convergence_state(
    selected: &HashMap<String, ContextHit>,
    limit: usize,
    previous_topk: &mut Vec<String>,
    stable_rounds: &mut u32,
    max_convergence_rounds: u32,
) -> bool {
    let mut candidate = selected.values().cloned().collect::<Vec<_>>();
    candidate.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    let topk = candidate
        .iter()
        .take(limit)
        .map(|hit| hit.uri.clone())
        .collect::<Vec<_>>();

    if topk == *previous_topk {
        *stable_rounds = (*stable_rounds).saturating_add(1);
    } else {
        *stable_rounds = 0;
    }
    *previous_topk = topk;
    *stable_rounds >= max_convergence_rounds
}

fn initialize_query_frontier(
    config: &DrrConfig,
    index: &InMemoryIndex,
    options: &SearchOptions,
    planned: &PlannedQuery,
    budget: ResolvedBudget,
    query: &str,
    limit: usize,
) -> QueryInitialization {
    let target = options.target_uri.clone();
    let filter = options.filter.as_ref();
    let filter_projection = index.filter_projection_uris(filter);
    let root_records = if let Some(target_uri) = target.as_ref() {
        index
            .get(&target_uri.to_string())
            .into_iter()
            .filter(|record| record.depth <= budget.depth)
            .filter(|record| uri_matches_filter_projection(&record.uri, filter_projection.as_ref()))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        index
            .scope_roots(&planned.scopes)
            .into_iter()
            .filter(|record| record.depth <= budget.depth)
            .filter(|record| uri_matches_filter_projection(&record.uri, filter_projection.as_ref()))
            .collect::<Vec<_>>()
    };
    let mut global_dirs =
        index.search_directories(query, target.as_ref(), config.global_topk, filter);
    global_dirs.retain(|x| {
        uri_matches_query_bounds(&x.record.uri, planned, target.as_ref())
            && x.record.depth <= budget.depth
    });

    let mut global_rank = index.search(
        query,
        target.as_ref(),
        limit.max(32),
        options.score_threshold,
        filter,
    );
    global_rank.retain(|x| {
        uri_matches_query_bounds(&x.record.uri, planned, target.as_ref())
            && x.record.depth <= budget.depth
    });

    let score_map = global_rank
        .iter()
        .map(|scored| (scored.record.uri.clone(), scored.score))
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

    QueryInitialization {
        trace_start,
        frontier,
        score_map,
        global_rank,
        filter_projection,
    }
}

fn finalize_single_query_run(input: FinalizeSingleQueryInput<'_>) -> SingleRunResult {
    let FinalizeSingleQueryInput {
        mut selected,
        global_rank,
        limit,
        trace_id,
        options,
        query,
        target,
        trace_start,
        steps,
        stop_reason,
        explored,
        stable_rounds,
        latency_ms,
    } = input;
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
        .map(|hit| TracePoint {
            uri: hit.uri.clone(),
            score: hit.score,
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
            latency_ms,
            explored_nodes: explored,
            convergence_rounds: stable_rounds,
            typed_query_count: 1,
            relation_enriched_hits: 0,
            relation_enriched_links: 0,
        },
    };
    SingleRunResult { hits, trace }
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
