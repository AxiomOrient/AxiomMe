use std::path::Path;
#[cfg(feature = "host-tools")]
use std::process::Command;

pub const AXIOMME_HOST_TOOLS_ENV: &str = "AXIOMME_HOST_TOOLS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostToolsMode {
    Enabled,
    Disabled,
}

impl HostToolsMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostToolsPolicySource {
    Environment,
    TargetDefault,
}

impl HostToolsPolicySource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "env",
            Self::TargetDefault => "target_default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostToolsPolicy {
    pub mode: HostToolsMode,
    pub source: HostToolsPolicySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCommandSpec<'a> {
    pub operation: &'a str,
    pub program: &'a str,
    pub args: &'a [&'a str],
    pub current_dir: Option<&'a Path>,
}

impl<'a> HostCommandSpec<'a> {
    #[must_use]
    pub const fn new(operation: &'a str, program: &'a str, args: &'a [&'a str]) -> Self {
        Self {
            operation,
            program,
            args,
            current_dir: None,
        }
    }

    #[must_use]
    pub const fn with_current_dir(mut self, current_dir: &'a Path) -> Self {
        self.current_dir = Some(current_dir);
        self
    }
}

#[cfg_attr(
    not(feature = "host-tools"),
    allow(
        dead_code,
        reason = "result shape stays stable across feature profiles"
    )
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCommandResult {
    Blocked {
        reason: String,
    },
    SpawnError {
        error: String,
    },
    Completed {
        success: bool,
        stdout: String,
        stderr: String,
    },
}

#[must_use]
pub fn resolve_host_tools_policy() -> HostToolsPolicy {
    let env_raw = std::env::var(AXIOMME_HOST_TOOLS_ENV).ok();
    resolve_host_tools_policy_with(env_raw.as_deref(), cfg!(target_os = "ios"))
}

#[must_use]
pub fn run_host_command(spec: HostCommandSpec<'_>) -> HostCommandResult {
    run_host_command_with_policy(spec, resolve_host_tools_policy())
}

#[must_use]
fn run_host_command_with_policy(
    spec: HostCommandSpec<'_>,
    policy: HostToolsPolicy,
) -> HostCommandResult {
    if policy.mode == HostToolsMode::Disabled {
        return HostCommandResult::Blocked {
            reason: format_host_tools_block_reason(spec.operation, policy),
        };
    }

    #[cfg(feature = "host-tools")]
    {
        let mut command = Command::new(spec.program);
        command.args(spec.args);
        if let Some(current_dir) = spec.current_dir {
            command.current_dir(current_dir);
        }

        match command.output() {
            Ok(output) => HostCommandResult::Completed {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            },
            Err(err) => HostCommandResult::SpawnError {
                error: err.to_string(),
            },
        }
    }

    #[cfg(not(feature = "host-tools"))]
    {
        let _ = policy;
        HostCommandResult::Blocked {
            reason: format_host_tools_feature_disabled_reason(spec.operation),
        }
    }
}

fn format_host_tools_block_reason(operation: &str, policy: HostToolsPolicy) -> String {
    format!(
        "host_tools_disabled operation={operation} mode={} source={} env={} (set env to on/1/true to enable host process execution)",
        policy.mode.as_str(),
        policy.source.as_str(),
        AXIOMME_HOST_TOOLS_ENV
    )
}

#[cfg(not(feature = "host-tools"))]
fn format_host_tools_feature_disabled_reason(operation: &str) -> String {
    format!(
        "host_tools_unavailable operation={operation} feature=host-tools (compile axiomme-core with feature `host-tools` to enable host process execution)"
    )
}

#[must_use]
fn resolve_host_tools_policy_with(env_raw: Option<&str>, target_is_ios: bool) -> HostToolsPolicy {
    if let Some(mode) = env_raw.and_then(parse_host_tools_mode) {
        return HostToolsPolicy {
            mode,
            source: HostToolsPolicySource::Environment,
        };
    }

    let mode = if target_is_ios {
        HostToolsMode::Disabled
    } else {
        HostToolsMode::Enabled
    };
    HostToolsPolicy {
        mode,
        source: HostToolsPolicySource::TargetDefault,
    }
}

#[must_use]
fn parse_host_tools_mode(raw: &str) -> Option<HostToolsMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Some(HostToolsMode::Enabled),
        "0" | "false" | "no" | "off" | "disabled" | "none" => Some(HostToolsMode::Disabled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_tools_mode_accepts_common_tokens() {
        assert_eq!(parse_host_tools_mode("on"), Some(HostToolsMode::Enabled));
        assert_eq!(parse_host_tools_mode("1"), Some(HostToolsMode::Enabled));
        assert_eq!(parse_host_tools_mode("off"), Some(HostToolsMode::Disabled));
        assert_eq!(parse_host_tools_mode("0"), Some(HostToolsMode::Disabled));
    }

    #[test]
    fn policy_prefers_environment_override() {
        let policy = resolve_host_tools_policy_with(Some("off"), false);
        assert_eq!(
            policy,
            HostToolsPolicy {
                mode: HostToolsMode::Disabled,
                source: HostToolsPolicySource::Environment,
            }
        );
    }

    #[test]
    fn policy_defaults_to_disabled_on_ios_target() {
        let policy = resolve_host_tools_policy_with(None, true);
        assert_eq!(policy.mode, HostToolsMode::Disabled);
        assert_eq!(policy.source, HostToolsPolicySource::TargetDefault);
    }

    #[test]
    fn policy_defaults_to_enabled_on_non_ios_target() {
        let policy = resolve_host_tools_policy_with(None, false);
        assert_eq!(policy.mode, HostToolsMode::Enabled);
        assert_eq!(policy.source, HostToolsPolicySource::TargetDefault);
    }

    #[test]
    fn run_host_command_with_disabled_policy_returns_blocked() {
        let spec = HostCommandSpec::new("test:blocked", "this-command-should-not-run", &[]);
        let result = run_host_command_with_policy(
            spec,
            HostToolsPolicy {
                mode: HostToolsMode::Disabled,
                source: HostToolsPolicySource::TargetDefault,
            },
        );
        match result {
            HostCommandResult::Blocked { reason } => {
                assert!(reason.contains("operation=test:blocked"));
            }
            other => panic!("expected Blocked result, got: {other:?}"),
        }
    }

    #[cfg(feature = "host-tools")]
    #[test]
    fn run_host_command_with_enabled_policy_reports_spawn_error() {
        let spec = HostCommandSpec::new("test:spawn_error", "axiomme-command-does-not-exist", &[]);
        let result = run_host_command_with_policy(
            spec,
            HostToolsPolicy {
                mode: HostToolsMode::Enabled,
                source: HostToolsPolicySource::TargetDefault,
            },
        );
        match result {
            HostCommandResult::SpawnError { error } => assert!(!error.trim().is_empty()),
            other => panic!("expected SpawnError result, got: {other:?}"),
        }
    }

    #[cfg(all(unix, feature = "host-tools"))]
    #[test]
    fn run_host_command_with_enabled_policy_reports_success_output() {
        let spec = HostCommandSpec::new("test:success", "sh", &["-c", "printf ok"]);
        let result = run_host_command_with_policy(
            spec,
            HostToolsPolicy {
                mode: HostToolsMode::Enabled,
                source: HostToolsPolicySource::TargetDefault,
            },
        );
        match result {
            HostCommandResult::Completed {
                success,
                stdout,
                stderr: _,
            } => {
                assert!(success);
                assert_eq!(stdout, "ok");
            }
            other => panic!("expected Completed result, got: {other:?}"),
        }
    }

    #[cfg(all(unix, feature = "host-tools"))]
    #[test]
    fn run_host_command_with_enabled_policy_reports_failure_status() {
        let spec = HostCommandSpec::new("test:failure", "sh", &["-c", "exit 7"]);
        let result = run_host_command_with_policy(
            spec,
            HostToolsPolicy {
                mode: HostToolsMode::Enabled,
                source: HostToolsPolicySource::TargetDefault,
            },
        );
        match result {
            HostCommandResult::Completed {
                success,
                stdout: _,
                stderr: _,
            } => assert!(!success),
            other => panic!("expected Completed result, got: {other:?}"),
        }
    }

    #[cfg(not(feature = "host-tools"))]
    #[test]
    fn run_host_command_with_enabled_policy_is_blocked_when_feature_is_disabled() {
        let spec = HostCommandSpec::new("test:feature-off", "sh", &["-c", "printf ok"]);
        let result = run_host_command_with_policy(
            spec,
            HostToolsPolicy {
                mode: HostToolsMode::Enabled,
                source: HostToolsPolicySource::TargetDefault,
            },
        );
        match result {
            HostCommandResult::Blocked { reason } => {
                assert!(reason.contains("host_tools_unavailable"));
                assert!(reason.contains("operation=test:feature-off"));
            }
            other => panic!("expected Blocked result, got: {other:?}"),
        }
    }
}
