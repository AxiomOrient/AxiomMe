use clap::Args;

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Local file/dir path or HTTP(S) URL to ingest.
    pub source: String,
    /// Destination root URI (directory semantics). Source filename is preserved.
    #[arg(
        long,
        long_help = "Destination root URI (directory semantics). When source is a file, its filename is preserved under this URI."
    )]
    pub target: Option<String>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub wait: bool,
    /// Keep only markdown files (`.md`, `.markdown`) during ingest.
    #[arg(long, default_value_t = false)]
    pub markdown_only: bool,
    /// Exclude source paths by glob pattern (requires `--markdown-only`).
    #[arg(long = "exclude", value_name = "GLOB")]
    pub exclude: Vec<String>,
    /// Include hidden files/directories when `--markdown-only` is enabled.
    #[arg(long, default_value_t = false)]
    pub include_hidden: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    pub uri: String,
    #[arg(short, long)]
    pub recursive: bool,
}

#[derive(Debug, Args)]
pub struct GlobArgs {
    pub pattern: String,
    #[arg(long)]
    pub uri: Option<String>,
}

#[derive(Debug, Args)]
pub struct UriArg {
    pub uri: String,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    pub uri: String,
    #[arg(long, default_value_t = false)]
    pub recursive: bool,
}

#[derive(Debug, Args)]
pub struct MoveArgs {
    pub from_uri: String,
    pub to_uri: String,
}
#[derive(Debug, Args)]
pub struct FindArgs {
    #[arg(allow_hyphen_values = true)]
    pub query: String,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
    #[arg(long)]
    pub budget_ms: Option<u64>,
    #[arg(long)]
    pub budget_nodes: Option<usize>,
    #[arg(long)]
    pub budget_depth: Option<usize>,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[arg(allow_hyphen_values = true)]
    pub query: String,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
    /// Drop hits whose normalized score is below this threshold.
    #[arg(long, value_parser = parse_score_threshold)]
    pub score_threshold: Option<f32>,
    /// Require this many query tokens to appear in each hit (`>=2`).
    #[arg(long, value_parser = parse_min_match_tokens)]
    pub min_match_tokens: Option<usize>,
    #[arg(long)]
    pub budget_ms: Option<u64>,
    #[arg(long)]
    pub budget_nodes: Option<usize>,
    #[arg(long)]
    pub budget_depth: Option<usize>,
}
#[derive(Debug, Args)]
pub struct ReconcileArgs {
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long = "scope")]
    pub scopes: Vec<String>,
    #[arg(long, default_value_t = 50)]
    pub max_drift_sample: usize,
}
#[derive(Debug, Args)]
pub struct ExportArgs {
    pub uri: String,
    pub to: String,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    pub file: String,
    pub parent: String,
    #[arg(long, default_value_t = false)]
    pub force: bool,
    #[arg(long, default_value_t = true)]
    pub vectorize: bool,
}

#[derive(Debug, Args)]
pub struct WebArgs {
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long, default_value_t = 8787)]
    pub port: u16,
}

fn parse_score_threshold(raw: &str) -> std::result::Result<f32, String> {
    let value = raw
        .parse::<f32>()
        .map_err(|_| format!("invalid float value '{raw}'"))?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!(
            "score threshold must be within [0.0, 1.0], got {value}"
        ));
    }
    Ok(value)
}

fn parse_min_match_tokens(raw: &str) -> std::result::Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("invalid integer value '{raw}'"))?;
    if value < 2 {
        return Err(format!("min-match-tokens must be >= 2, got {value}"));
    }
    Ok(value)
}
