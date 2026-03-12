use crate::models::SearchBudget;

use super::config::DrrConfig;

#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedBudget {
    pub time_ms: Option<u64>,
    pub nodes: usize,
    pub depth: usize,
}

pub(super) fn resolve_budget(config: &DrrConfig, budget: Option<&SearchBudget>) -> ResolvedBudget {
    let nodes = budget
        .and_then(|x| x.max_nodes)
        .unwrap_or(config.max_nodes)
        .max(1);
    let depth = budget
        .and_then(|x| x.max_depth)
        .unwrap_or(config.max_depth)
        .max(1);
    let time_ms = budget.and_then(|x| x.max_ms);
    ResolvedBudget {
        time_ms,
        nodes,
        depth,
    }
}
