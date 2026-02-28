use std::fs;
use std::path::PathBuf;

use crate::error::{AxiomError, Result};

pub(super) fn resolve_workspace_dir(workspace_dir: Option<&str>) -> Result<PathBuf> {
    let input = workspace_dir.unwrap_or(".");
    let raw = PathBuf::from(input);
    let absolute = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()?.join(raw)
    };
    if !absolute.exists() {
        return Err(AxiomError::NotFound(format!(
            "workspace directory not found: {}",
            absolute.display()
        )));
    }
    let workspace = fs::canonicalize(absolute)?;
    if !workspace.join("Cargo.toml").exists() {
        return Err(AxiomError::Validation(format!(
            "workspace missing Cargo.toml: {}",
            workspace.display()
        )));
    }
    Ok(workspace)
}
