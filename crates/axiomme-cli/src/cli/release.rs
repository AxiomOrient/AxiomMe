use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ReleaseArgs {
    #[command(subcommand)]
    pub command: ReleaseCommand,
}
#[derive(Debug, Subcommand)]
pub enum ReleaseCommand {
    Pack {
        #[arg(long)]
        workspace_dir: Option<String>,
        #[arg(long, default_value_t = 100)]
        replay_limit: usize,
        #[arg(long, default_value_t = 8)]
        replay_max_cycles: u32,
        #[arg(long, default_value_t = 200)]
        trace_limit: usize,
        #[arg(long, default_value_t = 200)]
        request_limit: usize,
        #[arg(long, default_value_t = 200)]
        eval_trace_limit: usize,
        #[arg(long, default_value_t = 50)]
        eval_query_limit: usize,
        #[arg(long, default_value_t = 10)]
        eval_search_limit: usize,
        #[arg(long, default_value_t = 60)]
        benchmark_query_limit: usize,
        #[arg(long, default_value_t = 10)]
        benchmark_search_limit: usize,
        #[arg(long, default_value_t = 600)]
        benchmark_threshold_p95_ms: u128,
        #[arg(long, default_value_t = 0.75)]
        benchmark_min_top1_accuracy: f32,
        #[arg(long)]
        benchmark_min_stress_top1_accuracy: Option<f32>,
        #[arg(long)]
        benchmark_max_p95_regression_pct: Option<f32>,
        #[arg(long)]
        benchmark_max_top1_regression_pct: Option<f32>,
        #[arg(long, default_value_t = 1)]
        benchmark_window_size: usize,
        #[arg(long, default_value_t = 1)]
        benchmark_required_passes: usize,
        #[arg(long, default_value = "strict")]
        security_audit_mode: String,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}
