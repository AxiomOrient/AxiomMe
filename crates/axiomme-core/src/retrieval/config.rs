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
