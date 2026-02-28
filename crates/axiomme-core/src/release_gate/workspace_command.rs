use std::path::Path;

use crate::host_tools::{HostCommandResult, HostCommandSpec, run_host_command};
use crate::text::{OutputTrimMode, first_non_empty_output};

pub(super) fn run_workspace_command(
    workspace_dir: &Path,
    cmd: &str,
    args: &[&str],
) -> (bool, String) {
    #[cfg(test)]
    if let Some(mock) = run_workspace_command_mock(cmd, args) {
        return mock;
    }

    let operation = format!("release_gate:{cmd}");
    match run_host_command(
        HostCommandSpec::new(&operation, cmd, args).with_current_dir(workspace_dir),
    ) {
        HostCommandResult::Blocked { reason } => (false, reason),
        HostCommandResult::SpawnError { error } => (false, error),
        HostCommandResult::Completed {
            success,
            stdout,
            stderr,
        } => {
            let text = first_non_empty_output(&stdout, &stderr, OutputTrimMode::Preserve)
                .unwrap_or_default();
            (success, text)
        }
    }
}

#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
fn workspace_command_key(cmd: &str, args: &[&str]) -> String {
    if args.is_empty() {
        cmd.to_string()
    } else {
        format!("{cmd} {}", args.join(" "))
    }
}

#[cfg(test)]
fn run_workspace_command_mock(cmd: &str, args: &[&str]) -> Option<(bool, String)> {
    let key = workspace_command_key(cmd, args);
    WORKSPACE_COMMAND_MOCK_STORE.with(|store| store.borrow().get(&key).cloned())
}

#[cfg(test)]
thread_local! {
    static WORKSPACE_COMMAND_MOCK_STORE: RefCell<HashMap<String, (bool, String)>> =
        RefCell::new(HashMap::new());
}

#[cfg(test)]
struct WorkspaceCommandMockResetGuard {
    previous: HashMap<String, (bool, String)>,
}

#[cfg(test)]
impl WorkspaceCommandMockResetGuard {
    fn install(mocks: &[(&str, &[&str], bool, &str)]) -> Self {
        let mut current = HashMap::new();
        for (cmd, args, ok, output) in mocks {
            current.insert(
                workspace_command_key(cmd, args),
                (*ok, (*output).to_string()),
            );
        }
        let previous = WORKSPACE_COMMAND_MOCK_STORE
            .with(|store| std::mem::replace(&mut *store.borrow_mut(), current));
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for WorkspaceCommandMockResetGuard {
    fn drop(&mut self) {
        let mut previous = HashMap::new();
        std::mem::swap(&mut previous, &mut self.previous);
        WORKSPACE_COMMAND_MOCK_STORE.with(|store| {
            *store.borrow_mut() = previous;
        });
    }
}

#[cfg(test)]
pub(crate) fn with_workspace_command_mocks<T>(
    mocks: &[(&str, &[&str], bool, &str)],
    run: impl FnOnce() -> T,
) -> T {
    let _reset = WorkspaceCommandMockResetGuard::install(mocks);
    run()
}
