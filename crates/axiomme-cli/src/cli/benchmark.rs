use clap::{Args, Subcommand};

use super::parsers::{parse_min_one_usize, parse_non_negative_f32, parse_unit_interval_f32};

#[derive(Debug, Args)]
pub struct BenchmarkArgs {
    #[command(subcommand)]
    pub command: BenchmarkCommand,
}
#[derive(Debug, Subcommand)]
pub enum BenchmarkCommand {
    Run {
        #[arg(long, default_value_t = 100)]
        query_limit: usize,
        #[arg(long, default_value_t = 10)]
        search_limit: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_golden: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_trace: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_stress: bool,
        #[arg(long, default_value_t = false)]
        trace_expectations: bool,
        #[arg(long)]
        fixture_name: Option<String>,
    },
    Amortized {
        #[arg(long, default_value_t = 100)]
        query_limit: usize,
        #[arg(long, default_value_t = 10)]
        search_limit: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_golden: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_trace: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_stress: bool,
        #[arg(long, default_value_t = false)]
        trace_expectations: bool,
        #[arg(long)]
        fixture_name: Option<String>,
        #[arg(long, default_value_t = 3)]
        iterations: usize,
    },
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Trend {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Gate {
        #[arg(long, default_value_t = 600)]
        threshold_p95_ms: u128,
        #[arg(long, default_value_t = 0.75, value_parser = parse_unit_interval_f32)]
        min_top1_accuracy: f32,
        #[arg(long, value_parser = parse_unit_interval_f32)]
        min_stress_top1_accuracy: Option<f32>,
        #[arg(long, default_value = "custom")]
        gate_profile: String,
        #[arg(long, value_parser = parse_non_negative_f32)]
        max_p95_regression_pct: Option<f32>,
        #[arg(long, value_parser = parse_non_negative_f32)]
        max_top1_regression_pct: Option<f32>,
        #[arg(long, default_value_t = 1, value_parser = parse_min_one_usize)]
        window_size: usize,
        #[arg(long, default_value_t = 1, value_parser = parse_min_one_usize)]
        required_passes: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        record: bool,
        #[arg(long, default_value_t = false)]
        write_release_check: bool,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
    Fixture {
        #[command(subcommand)]
        command: BenchmarkFixtureCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum BenchmarkFixtureCommand {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 100)]
        query_limit: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_golden: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_trace: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_stress: bool,
        #[arg(long, default_value_t = false)]
        trace_expectations: bool,
    },
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}
