use std::path::Path;

use super::{gate_decision, run_workspace_command};
use crate::models::{
    BuildQualityGateDetails, ReleaseGateDecision, ReleaseGateDetails, ReleaseGateId,
};
use crate::text::truncate_text;

pub(super) fn evaluate_build_quality_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    let check = run_workspace_command(workspace_dir, "cargo", &["check", "--workspace"]);
    let fmt = run_workspace_command(workspace_dir, "cargo", &["fmt", "--all", "--check"]);
    let clippy = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    );
    let passed = check.0 && fmt.0 && clippy.0;
    let details = ReleaseGateDetails::BuildQuality(BuildQualityGateDetails {
        cargo_check: check.0,
        cargo_fmt: fmt.0,
        cargo_clippy: clippy.0,
        check_output: truncate_text(&check.1, 240),
        fmt_output: truncate_text(&fmt.1, 240),
        clippy_output: truncate_text(&clippy.1, 240),
    });
    gate_decision(ReleaseGateId::BuildQuality, passed, details, None)
}
