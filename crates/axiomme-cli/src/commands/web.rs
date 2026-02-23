use std::ffi::{OsStr, OsString};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy)]
pub(super) struct WebServeOptions<'a> {
    pub(super) host: &'a str,
    pub(super) port: u16,
}

pub(super) fn serve(root: &Path, options: WebServeOptions<'_>) -> Result<()> {
    let root = canonicalize_root(root);
    launch_external_viewer(&root, options)
}

fn launch_external_viewer(root: &Path, options: WebServeOptions<'_>) -> Result<()> {
    let candidate = viewer_binary_candidate();
    let candidate_name = candidate.to_string_lossy().to_string();

    let mut command = Command::new(&candidate);
    command
        .arg("--root")
        .arg(root.as_os_str())
        .arg("--host")
        .arg(options.host)
        .arg("--port")
        .arg(options.port.to_string())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            anyhow::bail!("external viewer '{candidate_name}' exited with status {status}")
        }
        Err(err) if err.kind() == ErrorKind::NotFound => anyhow::bail!(
            "external web viewer was not found ('{candidate_name}'). set AXIOMME_WEB_VIEWER_BIN or install axiomme-webd"
        ),
        Err(err) => {
            Err(err).with_context(|| format!("failed to launch external viewer '{candidate_name}'"))
        }
    }
}

fn viewer_binary_candidate() -> OsString {
    let configured = std::env::var_os("AXIOMME_WEB_VIEWER_BIN");
    resolve_viewer_binary_candidate(configured.as_deref())
}

fn resolve_viewer_binary_candidate(raw: Option<&OsStr>) -> OsString {
    if let Some(raw) = raw {
        let trimmed = raw.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return OsString::from(trimmed);
        }
    }
    OsString::from("axiomme-webd")
}

fn canonicalize_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::resolve_viewer_binary_candidate;

    #[test]
    fn viewer_binary_candidate_uses_default_when_unset() {
        assert_eq!(
            resolve_viewer_binary_candidate(None),
            OsStr::new("axiomme-webd")
        );
    }

    #[test]
    fn viewer_binary_candidate_uses_default_for_blank_value() {
        assert_eq!(
            resolve_viewer_binary_candidate(Some(OsStr::new("   "))),
            OsStr::new("axiomme-webd")
        );
    }

    #[test]
    fn viewer_binary_candidate_uses_trimmed_override() {
        assert_eq!(
            resolve_viewer_binary_candidate(Some(OsStr::new(" /tmp/viewer "))),
            OsStr::new("/tmp/viewer")
        );
    }
}
