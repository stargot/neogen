//! Per-tick instruction budget (backlog 2.3).
//!
//! # Mechanism (how the hook stops execution)
//!
//! Every script runs as its own Lua coroutine ([`mlua::Thread`]) with a
//! **per-thread debug hook** (`Thread::set_hook`) armed with
//! `HookTriggers::every_nth_instruction(hook_interval)`. The hook counts
//! fired intervals; when the tick's allowance is spent it returns
//! `Ok(VmState::Yield)` — supported by Lua 5.3+ — which suspends the
//! coroutine *at that exact instruction boundary*. No Lua error is raised:
//! the coroutine stays alive and `resume` continues from the same point
//! next tick. Budget exhaustion is reported to Rust as
//! [`ScriptError::BudgetExceeded`]; it is control flow, not a crash.
//!
//! The budget is enforced with `hook_interval` granularity: `consumed` is
//! a multiple of the interval (still deterministic — the Lua VM
//! instruction stream of a fixed script is fixed).
//!
//! # One state per Runtime, scripts take turns (decision for 2.6)
//!
//! All scripts of a world live on **one** Lua state owned by the
//! `Runtime`; each script is its own coroutine with its **own per-thread
//! hook and its own counter** — nothing is shared between scripts' hooks,
//! so no hook-rearming dance is needed and counters cannot mix. This was
//! chosen over a state-per-script: scripts share the sandboxed globals and
//! the future `move/scan/act/print` API (2.4) for free, and the scheduler
//! (2.6) just resumes coroutines in a deterministic order. (Scripts see
//! each other's globals today; 2.4 can give each script a private `_ENV`
//! copy if isolation is needed.)

use std::cell::RefCell;
use std::rc::Rc;

use mlua::thread::ThreadStatus;
use mlua::{HookTriggers, Lua, Thread, VmState};

use crate::{EvalValue, ScriptError};

/// Default instructions per tick (backlog-suggested 100k).
pub const DEFAULT_INSTRUCTIONS_PER_TICK: u32 = 100_000;
/// Default hook interval: the hook fires every 512 VM instructions —
/// small enough for tight budgets, large enough to keep the overhead low.
pub const DEFAULT_HOOK_INTERVAL: u32 = 512;

/// Runtime tuning knobs (passed to `Runtime::new`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Instructions a script may execute per tick before suspension.
    pub instructions_per_tick: u32,
    /// How often (in VM instructions) the counting hook fires.
    pub hook_interval: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            instructions_per_tick: DEFAULT_INSTRUCTIONS_PER_TICK,
            hook_interval: DEFAULT_HOOK_INTERVAL,
        }
    }
}

impl RuntimeConfig {
    /// Intervals the hook may consume per tick (at least one, so tiny
    /// budgets still stop the script instead of running unchecked).
    fn intervals_per_tick(&self) -> u32 {
        (self.instructions_per_tick / self.hook_interval.max(1)).max(1)
    }
}

/// Shared state between the driver and the hook closure of one script.
#[derive(Debug)]
struct HookBudget {
    intervals_left: u32,
    exceeded: bool,
}

type SharedBudget = Rc<RefCell<HookBudget>>;

/// Outcome of resuming a script for one tick.
#[derive(Debug, Clone, PartialEq)]
pub enum TickOutcome {
    /// The script finished (its return value is carried).
    Completed(EvalValue),
    /// The script yielded on its own (`coroutine.yield`); resume next tick.
    Yielded(EvalValue),
}

/// A player script: one coroutine plus its budget.
///
/// Resumed tick by tick via [`Script::resume_tick`]; after
/// [`ScriptError::BudgetExceeded`] the very same call continues execution
/// from the suspension point.
#[derive(Debug)]
pub struct Script {
    id: u32,
    thread: Thread,
    budget: SharedBudget,
    config: RuntimeConfig,
    tick: u64,
}

impl Script {
    pub(crate) fn spawn(
        lua: &Lua,
        config: RuntimeConfig,
        id: u32,
        source: &str,
    ) -> Result<Self, ScriptError> {
        let function = lua.load(source).into_function()?;
        Self::spawn_function(lua, config, id, function)
    }

    /// Spawn from an already-built function (e.g. a chunk loaded with a
    /// custom `_ENV` — see `ScriptHost::create_script`).
    pub(crate) fn spawn_function(
        lua: &Lua,
        config: RuntimeConfig,
        id: u32,
        function: mlua::Function,
    ) -> Result<Self, ScriptError> {
        let thread = lua.create_thread(function)?;
        let budget = install_hook(&thread, config)?;
        Ok(Self {
            id,
            thread,
            budget,
            config,
            tick: 0,
        })
    }

    /// Script id (sequential, issued by the `Runtime`).
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Number of tick-resumes performed on this script so far.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Whether the coroutine has finished (successfully or with an error).
    pub fn is_finished(&self) -> bool {
        matches!(
            self.thread.status(),
            ThreadStatus::Finished | ThreadStatus::Error
        )
    }

    /// Resume the script for one tick.
    ///
    /// - `Ok(Completed(v))` — the script ran to its end, returning `v`.
    /// - `Ok(Yielded(v))` — the script itself called `coroutine.yield(v)`.
    /// - `Err(BudgetExceeded)` — the tick allowance is spent; the coroutine
    ///   is suspended, not killed: the next `resume_tick` continues it.
    /// - `Err(Lua(_))` — a real script error; the coroutine is dead.
    pub fn resume_tick(&mut self) -> Result<TickOutcome, ScriptError> {
        // Re-arm the budget for this tick.
        {
            let mut budget = self.budget.borrow_mut();
            budget.intervals_left = self.config.intervals_per_tick();
            budget.exceeded = false;
        }

        self.tick = self.tick.saturating_add(1);
        let outcome = match self.thread.resume::<mlua::Value>(()) {
            Ok(value) => {
                let exceeded = self.budget.borrow().exceeded;
                if exceeded {
                    // The yield happens at the interval boundary *after* the
                    // allowance is spent — the (n+1)-th firing.
                    let consumed = u64::from(self.config.intervals_per_tick() + 1)
                        * u64::from(self.config.hook_interval.max(1));
                    return Err(ScriptError::BudgetExceeded {
                        script_id: self.id,
                        tick: self.tick,
                        consumed,
                    });
                }
                if self.is_finished() {
                    TickOutcome::Completed(value.into())
                } else {
                    TickOutcome::Yielded(value.into())
                }
            }
            Err(error) => return Err(ScriptError::Lua(error)),
        };
        Ok(outcome)
    }
}

/// Arm a per-thread counting hook on a coroutine.
fn install_hook(thread: &Thread, config: RuntimeConfig) -> Result<SharedBudget, ScriptError> {
    let budget = Rc::new(RefCell::new(HookBudget {
        intervals_left: config.intervals_per_tick(),
        exceeded: false,
    }));
    let hook_budget = Rc::clone(&budget);
    let triggers = HookTriggers {
        every_nth_instruction: Some(config.hook_interval.max(1)),
        ..HookTriggers::default()
    };
    thread.set_hook(triggers, move |_lua, _debug| {
        let mut budget = hook_budget.borrow_mut();
        if budget.intervals_left == 0 {
            // Suspend the coroutine at this instruction boundary —
            // resumable, not an error (see module docs).
            budget.exceeded = true;
            return Ok(VmState::Yield);
        }
        budget.intervals_left -= 1;
        Ok(VmState::Continue)
    })?;
    Ok(budget)
}
