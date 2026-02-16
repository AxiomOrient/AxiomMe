use std::process::Command;
use std::{env, path::PathBuf};

use tempfile::tempdir;

fn cli_bin_path() -> PathBuf {
    if let Ok(path) = env::var("CARGO_BIN_EXE_axiomme-cli") {
        return PathBuf::from(path);
    }
    if let Ok(path) = env::var("CARGO_BIN_EXE_axiomme_cli") {
        return PathBuf::from(path);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("workspace root");
    let bin_name = if cfg!(windows) {
        "axiomme-cli.exe"
    } else {
        "axiomme-cli"
    };
    let fallback = workspace_root.join("target").join("debug").join(bin_name);
    assert!(
        fallback.exists(),
        "axiomme-cli binary not found at {}",
        fallback.display()
    );
    fallback
}

#[test]
fn queue_status_process_contract_returns_success_with_json_payload() {
    // Pseudocode:
    // Given a fresh root
    // When running `axiomme-cli queue status`
    // Then process exits with success and emits queue JSON payload.
    let root = tempdir().expect("tempdir");
    let output = Command::new(cli_bin_path())
        .args([
            "--root",
            root.path().to_str().expect("root path"),
            "queue",
            "status",
        ])
        .output()
        .expect("run queue status");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"counts\""));
    assert!(stdout.contains("\"lanes\""));
}

#[test]
fn benchmark_gate_enforce_process_contract_returns_non_zero_on_gate_failure() {
    // Pseudocode:
    // Given no benchmark reports
    // When running `axiomme-cli benchmark gate --enforce`
    // Then process exits non-zero and exposes enforce failure reason.
    let root = tempdir().expect("tempdir");
    let output = Command::new(cli_bin_path())
        .args([
            "--root",
            root.path().to_str().expect("root path"),
            "benchmark",
            "gate",
            "--enforce",
        ])
        .output()
        .expect("run benchmark gate enforce");

    assert!(
        !output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("benchmark gate failed"));
}
