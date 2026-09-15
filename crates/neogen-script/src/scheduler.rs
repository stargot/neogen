//! Multi-script scheduler (backlog 2.6).
//!
//! # Tick phasing
//!
//! One `step_world` is exactly two phases, in this order:
//!
//! 1. **Execution phase** — every *alive* managed script is resumed in
//!    ascending script-id order, each with its own per-tick instruction
//!    budget (2.3). Scripts observe the world as it was at the *start* of
//!    the tick and queue commands through the API.
//! 2. **World phase** — `world.step()`: the core applies the queued
//!    commands and advances physics, executing each rover's queue in
//!    ascending rover-id order (core guarantee since 1.5). Commands a
//!    script queued during phase 1 therefore land in the *same* tick.
//!
//! This is the genre contract «scripts work while you plan»: scripts plan,
//! the world executes — once per tick, in a fixed order.
//!
//! # Determinism guarantee
//!
//! The result of a tick depends **only** on (the input world, the script
//! sources, the script-id order). No wall clock, no thread scheduling, no
//! `HashMap` iteration order in Rust code. Note for script *authors*:
//! iteration order of Lua tables (`pairs`/`next`) is not specified by the
//! language — scripts that depend on it may observe different orderings;
//! sort keys before iterating if order matters.
//!
//! Failure policy is inherited from 2.5: a script error fails only that
//! script and is logged; budget exhaustion is a pause; dead coroutines are
//! never resumed.

use std::collections::BTreeMap;

use crate::api::SharedContext;
use crate::logbuffer::LogEntry;
use crate::{ScriptContext as _, ScriptError, ScriptState, TickOutcome};

/// A script registered with the host's tick loop (see
/// `ScriptHost::attach_script`).
pub(crate) struct ManagedScript {
    pub(crate) script: crate::Script,
    pub(crate) rover: neogen_core::RoverId,
    /// Source text (kept for the future save system, backlog 9.x).
    #[allow(dead_code)]
    pub(crate) source: String,
    pub(crate) state: ScriptState,
}

/// Run one full tick: execution phase (resume alive scripts, ascending
/// id), then world phase (`world.step()`).
pub(crate) fn run_tick(managed: &mut BTreeMap<u32, ManagedScript>, context: &SharedContext) {
    // ---- Phase 1: execution ----
    let ids: Vec<u32> = managed.keys().copied().collect();
    for id in ids {
        // Only alive scripts are resumed: a dead coroutine (Finished or
        // Failed) would error with "non-resumable" on every tick.
        if !managed[&id].state.is_alive() {
            continue;
        }
        let rover = managed[&id].rover;
        let outcome = managed
            .get_mut(&id)
            .expect("id taken from the map")
            .script
            .resume_tick();
        let state = match outcome {
            Ok(TickOutcome::Completed(_)) => ScriptState::Finished,
            Ok(TickOutcome::Yielded(_)) => ScriptState::Running,
            Err(ScriptError::BudgetExceeded { .. }) => ScriptState::SuspendedBudget,
            Err(error) => {
                // Compile cannot occur here (scripts compile at build);
                // anything else is a script failure: log it, fail only
                // this script.
                let entry = LogEntry {
                    tick: context.borrow().current_tick(),
                    script_id: id,
                    rover_id: rover,
                    text: crate::errors::log_line(&error),
                };
                context.borrow_mut().log(entry);
                ScriptState::Failed(error)
            }
        };
        if let Some(managed_script) = managed.get_mut(&id) {
            managed_script.state = state;
        }
    }

    // ---- Phase 2: world ----
    context.borrow_mut().world_mut().step();
}
