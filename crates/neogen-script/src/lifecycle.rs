//! Script lifecycle states (backlog 2.5).

use crate::ScriptError;

/// Lifecycle state of a managed script.
///
/// Failure policy (MVP): a script error stops **only that script** — the
/// world, the tick loop and every other script keep running. Restarting a
/// failed script is **manual by design** (`ScriptHost::restart_script`);
/// automatic restart policies (backoff, budgets) may arrive post-MVP —
/// this is a policy decision, not a missing feature.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptState {
    /// Alive; will be resumed on the next `step_world`.
    Running,
    /// Alive, but the instruction budget for the current tick is spent —
    /// a pause, **not** a failure. Continues next tick from the same spot.
    SuspendedBudget,
    /// Ran to completion (its return value is gone; console output lives
    /// in the log buffer).
    Finished,
    /// Died on a script error; stays down until a manual restart.
    Failed(ScriptError),
    /// Stopped by the host (`stop_script`, phase 5.4 Run/Stop): the
    /// coroutine is parked and never resumed again; only a manual restart
    /// (with fresh source) revives the script id.
    Stopped,
}

impl ScriptState {
    /// Whether the script is still alive (will make progress).
    pub fn is_alive(&self) -> bool {
        matches!(self, Self::Running | Self::SuspendedBudget)
    }
}
