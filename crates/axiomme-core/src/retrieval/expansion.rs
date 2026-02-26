use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
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
use super::scoring::{make_hit, sort_hits_by_score_desc_uri_asc};

const GLOBAL_RANK_FLOOR_DEFAULT: usize = 128;
const GLOBAL_RANK_FLOOR_IDENTIFIER_QUERY: usize = 256;

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
    filter_projection: Option<HashSet<Arc<str>>>,
}

struct QueryCutoffs {
    score_threshold: Option<f32>,
    min_match_tokens: Option<usize>,
    query_tokens: HashSet<String>,
}

impl QueryCutoffs {
    fn from_options(query: &str, options: &SearchOptions) -> Self {
        let min_match_tokens = options.min_match_tokens.filter(|value| *value > 1);
        let query_tokens = if min_match_tokens.is_some() {
            crate::embedding::tokenize_set(query)
        } else {
            HashSet::new()
        };
        Self {
            score_threshold: options.score_threshold,
            min_match_tokens,
            query_tokens,
        }
    }

    fn allows_uri(&self, index: &InMemoryIndex, uri: &str, score: f32) -> bool {
        if let Some(threshold) = self.score_threshold
            && score < threshold
        {
            return false;
        }
        let Some(min_match_tokens) = self.min_match_tokens else {
            return true;
        };
        index.token_overlap_count(uri, &self.query_tokens) >= min_match_tokens
    }

    fn allows_scored_record(&self, index: &InMemoryIndex, scored: &ScoredRecord) -> bool {
        self.allows_uri(index, &scored.uri, scored.score)
    }
}

struct FinalizeRunContext<'a> {
    index: &'a InMemoryIndex,
    options: &'a SearchOptions,
    query: String,
    target: Option<AxiomUri>,
}

struct FinalizeRunCandidates<'a> {
    selected: HashMap<String, ContextHit>,
    global_rank: &'a [ScoredRecord],
    limit: usize,
}

struct FinalizeRunTrace {
    trace_id: String,
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
    filter_projection: Option<&'a HashSet<Arc<str>>>,
    query_cutoffs: &'a QueryCutoffs,
    limit: usize,
    score_map: &'a HashMap<String, f32>,
    frontier: BinaryHeap<Node>,
    run_start: Instant,
}

struct IdentifierFastPathInput<'a> {
    index: &'a InMemoryIndex,
    options: &'a SearchOptions,
    planned: &'a PlannedQuery,
    budget: ResolvedBudget,
    query: &'a str,
    query_cutoffs: &'a QueryCutoffs,
    limit: usize,
    run_start: Instant,
}

struct QueryFrontierInput<'a> {
    config: &'a DrrConfig,
    index: &'a InMemoryIndex,
    options: &'a SearchOptions,
    planned: &'a PlannedQuery,
    budget: ResolvedBudget,
    query: &'a str,
    query_cutoffs: &'a QueryCutoffs,
    limit: usize,
}

pub(super) fn run_single_query(
    config: &DrrConfig,
    index: &InMemoryIndex,
    options: &SearchOptions,
    planned: &PlannedQuery,
    budget: ResolvedBudget,
) -> SingleRunResult {
    let run_start = Instant::now();
    let query = planned.query.clone();
    let limit = options.limit.max(1);
    let query_cutoffs = QueryCutoffs::from_options(&query, options);
    if let Some(result) = run_identifier_query_fast_path(IdentifierFastPathInput {
        index,
        options,
        planned,
        budget,
        query: &query,
        query_cutoffs: &query_cutoffs,
        limit,
        run_start,
    }) {
        return result;
    }
    let target = options.target_uri.clone();
    let QueryInitialization {
        trace_start,
        frontier,
        score_map,
        global_rank,
        filter_projection,
    } = initialize_query_frontier(QueryFrontierInput {
        config,
        index,
        options,
        planned,
        budget,
        query: &query,
        query_cutoffs: &query_cutoffs,
        limit,
    });
    let loop_state = execute_expansion_loop(ExpansionLoopInput {
        config,
        index,
        planned,
        budget,
        target: target.as_ref(),
        filter_projection: filter_projection.as_ref(),
        query_cutoffs: &query_cutoffs,
        limit,
        score_map: &score_map,
        frontier,
        run_start,
    });
    let trace_id = Uuid::new_v4().to_string();

    finalize_single_query_run(
        FinalizeRunContext {
            index,
            options,
            query,
            target,
        },
        FinalizeRunCandidates {
            selected: loop_state.selected,
            global_rank: &global_rank,
            limit,
        },
        FinalizeRunTrace {
            trace_id,
            trace_start,
            steps: loop_state.steps,
            stop_reason: loop_state.stop_reason,
            explored: loop_state.explored,
            stable_rounds: loop_state.stable_rounds,
            latency_ms: run_start.elapsed().as_millis(),
        },
    )
}

fn run_identifier_query_fast_path(input: IdentifierFastPathInput<'_>) -> Option<SingleRunResult> {
    let IdentifierFastPathInput {
        index,
        options,
        planned,
        budget,
        query,
        query_cutoffs,
        limit,
        run_start,
    } = input;
    // max_ms/max_nodes are enforced by the bounded traversal loop. Fast path
    // bypasses that loop, so opt out when either bound is explicitly set.
    if let Some(budget) = options.budget.as_ref()
        && (budget.max_ms.is_some() || budget.max_nodes.is_some())
    {
        return None;
    }
    if !is_identifier_style_query(query) {
        return None;
    }

    let target = options.target_uri.clone();
    let mut ranked = index.search(
        query,
        target.as_ref(),
        limit.max(global_rank_floor(query)),
        options.score_threshold,
        options.filter.as_ref(),
    );
    ranked.retain(|item| {
        uri_matches_query_bounds(&item.uri, planned, target.as_ref())
            && item.depth <= budget.depth
            && query_cutoffs.allows_scored_record(index, item)
    });
    if ranked.is_empty() {
        return None;
    }
    let explored = ranked.len();

    let trace_id = Uuid::new_v4().to_string();
    let query_owned = query.to_string();
    let mut hits = ranked
        .iter()
        .take(limit)
        .filter_map(|item| make_hit_from_scored(index, item))
        .collect::<Vec<_>>();
    sort_hits_by_score_desc_uri_asc(&mut hits);
    let final_topk = hits
        .iter()
        .map(|hit| TracePoint {
            uri: hit.uri.clone(),
            score: hit.score,
        })
        .collect::<Vec<_>>();
    let start_points = ranked
        .iter()
        .take(3)
        .map(|item| TracePoint {
            uri: item.uri.clone(),
            score: item.score,
        })
        .collect::<Vec<_>>();
    let trace = RetrievalTrace {
        trace_id,
        request_type: options.request_type.clone(),
        query: query_owned,
        target_uri: target.as_ref().map(ToString::to_string),
        start_points,
        steps: Vec::new(),
        final_topk,
        stop_reason: "identifier_fast_path".to_string(),
        metrics: TraceStats {
            latency_ms: run_start.elapsed().as_millis(),
            explored_nodes: explored,
            convergence_rounds: 0,
            typed_query_count: 1,
            relation_enriched_hits: 0,
            relation_enriched_links: 0,
        },
    };
    Some(SingleRunResult { hits, trace })
}

fn execute_expansion_loop(input: ExpansionLoopInput<'_>) -> ExpansionLoopState {
    let ExpansionLoopInput {
        config,
        index,
        planned,
        budget,
        target,
        filter_projection,
        query_cutoffs,
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
                if query_cutoffs.allows_uri(index, &child.uri, propagated)
                    && let Some(record) = index.get(&child.uri)
                {
                    let hit = make_hit(record, propagated);
                    upsert_hit_if_higher(&mut selected, hit);
                    children_selected = children_selected.saturating_add(1);
                }
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

fn uri_matches_filter_projection(uri: &str, filter_projection: Option<&HashSet<Arc<str>>>) -> bool {
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
    let mut candidate = selected.values().collect::<Vec<_>>();
    candidate.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.uri.cmp(&b.uri))
    });
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

fn upsert_hit_if_higher(selected: &mut HashMap<String, ContextHit>, hit: ContextHit) {
    if let Some(existing) = selected.get_mut(&hit.uri) {
        if hit.score > existing.score {
            *existing = hit;
        }
        return;
    }
    selected.insert(hit.uri.clone(), hit);
}

fn initialize_query_frontier(input: QueryFrontierInput<'_>) -> QueryInitialization {
    let QueryFrontierInput {
        config,
        index,
        options,
        planned,
        budget,
        query,
        query_cutoffs,
        limit,
    } = input;
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
        uri_matches_query_bounds(&x.uri, planned, target.as_ref()) && x.depth <= budget.depth
    });

    let mut global_rank = index.search(
        query,
        target.as_ref(),
        limit.max(global_rank_floor(query)),
        options.score_threshold,
        filter,
    );
    global_rank.retain(|x| {
        uri_matches_query_bounds(&x.uri, planned, target.as_ref())
            && x.depth <= budget.depth
            && query_cutoffs.allows_scored_record(index, x)
    });

    let score_map = global_rank
        .iter()
        .map(|scored| (scored.uri.clone(), scored.score))
        .collect::<HashMap<_, _>>();
    let mut trace_start = Vec::new();
    let mut frontier = BinaryHeap::new();
    let mut seen_start = HashSet::new();
    for root in &root_records {
        let uri = root.uri.clone();
        if seen_start.insert(uri.clone()) {
            trace_start.push(TracePoint {
                uri: uri.clone(),
                score: 0.0,
            });
            frontier.push(Node {
                uri,
                score: 0.0,
                depth: root.depth,
            });
        }
    }
    for dir in &global_dirs {
        let uri = dir.uri.clone();
        if seen_start.insert(uri.clone()) {
            trace_start.push(TracePoint {
                uri: uri.clone(),
                score: dir.score,
            });
            frontier.push(Node {
                uri,
                score: dir.score,
                depth: dir.depth,
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

fn global_rank_floor(query: &str) -> usize {
    if is_identifier_style_query(query) {
        GLOBAL_RANK_FLOOR_IDENTIFIER_QUERY
    } else {
        GLOBAL_RANK_FLOOR_DEFAULT
    }
}

fn is_identifier_style_query(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains(char::is_whitespace) {
        return false;
    }
    let compact_len = trimmed.chars().filter(|ch| ch.is_alphanumeric()).count();
    compact_len >= 6
}

fn finalize_single_query_run(
    context: FinalizeRunContext<'_>,
    candidates: FinalizeRunCandidates<'_>,
    trace_input: FinalizeRunTrace,
) -> SingleRunResult {
    let FinalizeRunContext {
        index,
        options,
        query,
        target,
    } = context;
    let FinalizeRunCandidates {
        mut selected,
        global_rank,
        limit,
    } = candidates;
    let FinalizeRunTrace {
        trace_id,
        trace_start,
        steps,
        stop_reason,
        explored,
        stable_rounds,
        latency_ms,
    } = trace_input;
    // Merge a small global baseline to prevent small limits from overfitting
    // early DRR branch convergence.
    for scored in global_rank
        .iter()
        .filter(|scored| scored.is_leaf)
        .take(limit.max(8))
    {
        let Some(hit) = make_hit_from_scored(index, scored) else {
            continue;
        };
        upsert_hit_if_higher(&mut selected, hit);
    }
    let mut hits: Vec<_> = selected.into_values().collect();
    sort_hits_by_score_desc_uri_asc(&mut hits);
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

fn make_hit_from_scored(index: &InMemoryIndex, scored: &ScoredRecord) -> Option<ContextHit> {
    let record = index.get(&scored.uri)?;
    Some(make_hit(record, scored.score))
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

#[cfg(test)]
mod tests {
    use super::{
        GLOBAL_RANK_FLOOR_DEFAULT, GLOBAL_RANK_FLOOR_IDENTIFIER_QUERY, global_rank_floor,
        is_identifier_style_query, update_convergence_state, upsert_hit_if_higher,
    };
    use crate::models::ContextHit;
    use std::collections::HashMap;

    fn hit(uri: &str, score: f32) -> ContextHit {
        ContextHit {
            uri: uri.to_string(),
            score,
            abstract_text: String::new(),
            context_type: "resource".to_string(),
            relations: Vec::new(),
        }
    }

    #[test]
    fn identifier_style_query_detection_is_explicit() {
        assert!(is_identifier_style_query("projectlogbook"));
        assert!(is_identifier_style_query("mcpintegration"));
        assert!(!is_identifier_style_query("mcp integration"));
        assert!(!is_identifier_style_query("docs"));
    }

    #[test]
    fn global_rank_floor_uses_identifier_budget_for_dense_tokens() {
        assert_eq!(
            global_rank_floor("projectlogbook"),
            GLOBAL_RANK_FLOOR_IDENTIFIER_QUERY
        );
        assert_eq!(global_rank_floor("qa"), GLOBAL_RANK_FLOOR_DEFAULT);
        assert_eq!(global_rank_floor("qa guide"), GLOBAL_RANK_FLOOR_DEFAULT);
    }

    #[test]
    fn upsert_hit_if_higher_keeps_max_score_per_uri() {
        let mut selected = HashMap::new();
        upsert_hit_if_higher(&mut selected, hit("axiom://resources/docs/a.md", 0.21));
        upsert_hit_if_higher(&mut selected, hit("axiom://resources/docs/a.md", 0.39));
        upsert_hit_if_higher(&mut selected, hit("axiom://resources/docs/a.md", 0.22));

        assert_eq!(selected.len(), 1);
        assert!(
            (selected
                .get("axiom://resources/docs/a.md")
                .expect("selected hit")
                .score
                - 0.39)
                .abs()
                < 0.0001
        );
    }

    #[test]
    fn convergence_topk_is_deterministic_for_equal_scores() {
        let mut selected = HashMap::new();
        upsert_hit_if_higher(&mut selected, hit("axiom://resources/docs/b.md", 0.7));
        upsert_hit_if_higher(&mut selected, hit("axiom://resources/docs/a.md", 0.7));
        upsert_hit_if_higher(&mut selected, hit("axiom://resources/docs/c.md", 0.2));

        let mut previous_topk = Vec::new();
        let mut stable_rounds = 0;
        assert!(!update_convergence_state(
            &selected,
            2,
            &mut previous_topk,
            &mut stable_rounds,
            2
        ));
        assert_eq!(
            previous_topk,
            vec![
                "axiom://resources/docs/a.md".to_string(),
                "axiom://resources/docs/b.md".to_string()
            ]
        );
        assert_eq!(stable_rounds, 0);

        assert!(!update_convergence_state(
            &selected,
            2,
            &mut previous_topk,
            &mut stable_rounds,
            2
        ));
        assert_eq!(stable_rounds, 1);

        assert!(update_convergence_state(
            &selected,
            2,
            &mut previous_topk,
            &mut stable_rounds,
            2
        ));
        assert_eq!(stable_rounds, 2);
    }
}
