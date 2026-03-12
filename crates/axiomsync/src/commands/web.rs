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
    let mut last_not_found = None;

    for candidate in viewer_binary_candidates() {
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
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                anyhow::bail!("external viewer '{candidate_name}' exited with status {status}")
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                last_not_found = Some(candidate_name);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to launch external viewer '{candidate_name}'")
                });
            }
        }
    }

    let candidate_name = last_not_found.unwrap_or_else(|| "axiomsync-webd".to_string());
    anyhow::bail!(
        "external web viewer was not found ('{candidate_name}'). set AXIOMSYNC_WEB_VIEWER_BIN or install axiomsync-webd"
    )
}

fn viewer_binary_candidates() -> Vec<OsString> {
    let mut candidates = Vec::new();
    push_candidate(
        &mut candidates,
        std::env::var_os("AXIOMSYNC_WEB_VIEWER_BIN").as_deref(),
    );
    push_candidate(&mut candidates, Some(OsStr::new("axiomsync-webd")));
    candidates
}

fn push_candidate(candidates: &mut Vec<OsString>, raw: Option<&OsStr>) {
    let Some(candidate) = resolve_viewer_binary_candidate(raw) else {
        return;
    };
    if candidates.iter().any(|existing| existing == &candidate) {
        return;
    }
    candidates.push(candidate);
}

fn resolve_viewer_binary_candidate(raw: Option<&OsStr>) -> Option<OsString> {
    if let Some(raw) = raw {
        let trimmed = raw.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return Some(OsString::from(trimmed));
        }
    }
    None
}

fn canonicalize_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    use super::{resolve_viewer_binary_candidate, viewer_binary_candidates};

    #[test]
    fn viewer_binary_candidate_uses_default_when_unset() {
        assert_eq!(
            viewer_binary_candidates().first(),
            Some(&OsString::from("axiomsync-webd"))
        );
    }

    #[test]
    fn viewer_binary_candidate_ignores_blank_value() {
        assert_eq!(
            resolve_viewer_binary_candidate(Some(OsStr::new("   "))),
            None
        );
    }

    #[test]
    fn viewer_binary_candidate_uses_trimmed_override() {
        assert_eq!(
            resolve_viewer_binary_candidate(Some(OsStr::new(" /tmp/viewer "))),
            Some(OsString::from("/tmp/viewer"))
        );
    }
}
