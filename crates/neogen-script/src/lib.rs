//! Lua runtime for player scripts (backlog phase 2).
//!
//! `neogen-script` owns the Lua 5.4 runtime (via `mlua`, vendored — no
//! system Lua) and hosts the script sandbox (2.2), the per-tick instruction
//! budget (2.3), the `move/scan/act/print` API (2.4) and the multi-script
//! scheduler (2.6). Like `neogen-core`, this crate never depends on Godot
//! or GDExtension bindings: the simulation stays headless-testable, the
//! bridge consumes it from `neogen-gdext`.
//!
//! The runtime is deliberately **synchronous** (no mlua `async` feature):
//! scripts advance in discrete deterministic ticks, not on wall-clock I/O.

mod api;
mod budget;
mod logbuffer;
mod sandbox;

use core::fmt;
use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, LuaOptions};
use neogen_core::{RoverId, Vec2};

pub use api::{ACT_KINDS, ScriptContext, WorldContext};
pub use budget::{
    DEFAULT_HOOK_INTERVAL, DEFAULT_INSTRUCTIONS_PER_TICK, RuntimeConfig, Script, TickOutcome,
};
pub use logbuffer::{LogBuffer, LogEntry, MAX_LOG_BUFFER};

/// Failure inside the script runtime.
///
/// Phase 2.5 extends this into the full script-lifecycle error set.
#[derive(Debug)]
pub enum ScriptError {
    /// An error raised by Lua itself.
    Lua(mlua::Error),
    /// The per-tick instruction allowance is spent. The coroutine is
    /// *suspended, not killed* — the next `resume_tick` continues from the
    /// same instruction.
    BudgetExceeded {
        /// Id of the script (sequential, issued by the `Runtime`).
        script_id: u32,
        /// Tick number of the exhausted resume.
        tick: u64,
        /// Instructions consumed this tick (multiple of the hook interval).
        consumed: u64,
    },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lua(error) => write!(f, "{error}"),
            Self::BudgetExceeded {
                script_id,
                tick,
                consumed,
            } => write!(
                f,
                "script {script_id} exhausted its budget of instructions at tick {tick}                  ({consumed} consumed); suspended, not killed"
            ),
        }
    }
}

impl std::error::Error for ScriptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lua(error) => Some(error),
            _ => None,
        }
    }
}

impl From<mlua::Error> for ScriptError {
    fn from(error: mlua::Error) -> Self {
        Self::Lua(error)
    }
}

/// Result of evaluating a Lua chunk, mapped to a small crate-owned enum so
/// the public API does not leak `mlua` types (the bridge should not have
/// to depend on it).
#[derive(Debug, Clone, PartialEq)]
pub enum EvalValue {
    /// The chunk returned nothing (`nil`).
    Nil,
    /// A boolean.
    Bool(bool),
    /// A number (Lua numbers are f64; integers stay exact where possible).
    Number(f64),
    /// A string.
    String(String),
    /// Tables, functions, userdata, … — nothing the core needs to inspect.
    Other,
}

impl From<mlua::Value> for EvalValue {
    fn from(value: mlua::Value) -> Self {
        match value {
            mlua::Value::Nil => Self::Nil,
            mlua::Value::Boolean(b) => Self::Bool(b),
            mlua::Value::Integer(i) => Self::Number(i as f64),
            mlua::Value::Number(n) => Self::Number(n),
            mlua::Value::String(s) => Self::String(s.to_string_lossy()),
            _ => Self::Other,
        }
    }
}

/// Owner of the Lua state for one script host.
///
/// Intentionally cheap to extend: the sandbox globals (2.2), instruction
/// budget hooks (2.3) and API functions (2.4) will all live on the runtime
/// this struct owns — not in free functions.
pub struct Runtime {
    lua: Lua,
    config: RuntimeConfig,
    next_script_id: u32,
}

impl Runtime {
    /// Create a runtime with Lua 5.4 in the sandboxed environment (see
    /// [`sandbox`] for the policy) and the given budget configuration.
    ///
    /// Fallible on purpose: state creation and sandbox installation are
    /// part of the contract (2.1 reserved the `Result`).
    pub fn new(config: RuntimeConfig) -> Result<Self, ScriptError> {
        let lua = Lua::new_with(sandbox::safe_libs(), LuaOptions::default())?;
        sandbox::install(&lua)?;
        Ok(Self {
            lua,
            config,
            next_script_id: 0,
        })
    }

    /// Read-only visibility check into the sandboxed globals: does a
    /// script see a global with this name? For tests and bridge
    /// diagnostics — not a way for scripts to recover anything.
    pub fn has_global(&self, name: &str) -> bool {
        match self.lua.globals().get::<mlua::Value>(name) {
            Ok(value) => !value.is_nil(),
            Err(_) => false,
        }
    }

    /// Evaluate a chunk and return its `return` value.
    ///
    /// This is the raw escape hatch (tests, REPL-style experiments);
    /// player scripts will run through the tick-driven scheduler (2.3/2.6).
    /// Compile a player script into a budgeted coroutine (see
    /// [`budget`] for the mechanics). Compilation happens now: syntax
    /// errors surface before the first tick.
    pub fn create_script(&mut self, source: &str) -> Result<Script, ScriptError> {
        self.next_script_id = self.next_script_id.saturating_add(1);
        Script::spawn(&self.lua, self.config, self.next_script_id, source)
    }

    /// The runtime's budget configuration.
    pub fn config(&self) -> RuntimeConfig {
        self.config
    }

    pub fn eval(&self, chunk: &str) -> Result<EvalValue, ScriptError> {
        let value = self.lua.load(chunk).eval::<mlua::Value>()?;
        Ok(value.into())
    }
}

/// Host binding the Lua runtime to a real `neogen-core` world (backlog 2.4).
///
/// Owns one [`Runtime`] (Lua state) and one shared [`WorldContext`]
/// (world + log buffer). Scripts created here run with the sandboxed
/// whitelist in a **private environment table**: each script gets its own
/// `_ENV`, so scripts on the shared state do not see each other's globals
/// and cannot replace each other's API functions (the API is installed
/// per-script into its own env). `_G`/`_VERSION` are deliberately not
/// copied into script envs — there is no route back to the shared globals
/// table. Known limitation, documented: library tables (`math`, `string`,
/// …) are shared by reference, so a hostile script could still poison a
/// library table for everyone; per-script proxies are a later hardening
/// step if needed.
pub struct ScriptHost {
    runtime: Runtime,
    context: api::SharedContext,
}

impl ScriptHost {
    /// Create a host over a world with the given runtime config.
    pub fn new(world: neogen_core::World, config: RuntimeConfig) -> Result<Self, ScriptError> {
        Ok(Self {
            runtime: Runtime::new(config)?,
            context: Rc::new(RefCell::new(WorldContext::new(world))),
        })
    }

    /// Compile a script bound to a rover: sandboxed env + per-script API
    /// (`move/scan/act/print/scan_result`) + budgeted coroutine.
    pub fn create_script(&mut self, rover: RoverId, source: &str) -> Result<Script, ScriptError> {
        let lua = &self.runtime.lua;

        // Private environment: copy of the sandboxed globals (minus the
        // excluded keys), plus the per-script API.
        let env = lua.create_table()?;
        let globals = lua.globals();
        let mut copied = 0usize;
        globals.for_each(|key: mlua::LuaString, value: mlua::Value| {
            let name = key.to_string_lossy();
            if name == "_G" || name == "_VERSION" {
                return Ok(());
            }
            env.set(key, value)?;
            copied += 1;
            Ok(())
        })?;
        if copied == 0 {
            return Err(ScriptError::Lua(mlua::Error::runtime(
                "sandboxed globals are empty; refusing to build a script env",
            )));
        }

        let script_id = self.runtime.next_script_id.saturating_add(1);
        self.runtime.next_script_id = script_id;
        let context: Rc<RefCell<dyn ScriptContext>> = Rc::clone(&self.context) as _;
        api::install(lua, &env, context, rover, script_id)?;

        let function = lua.load(source).set_environment(env).into_function()?;
        Script::spawn_function(lua, self.runtime.config, script_id, function)
    }

    /// Advance the world by one tick (scripts are resumed separately).
    pub fn step_world(&mut self) {
        self.context.borrow_mut().world_mut().step();
    }

    /// Current world tick.
    pub fn world_tick(&self) -> u64 {
        self.context.borrow().world().tick()
    }

    /// Rover ids in the world, ascending.
    pub fn rover_ids(&self) -> Vec<RoverId> {
        let ctx = self.context.borrow();
        ctx.world().rovers().map(|rover| rover.id()).collect()
    }

    /// A rover's position.
    pub fn rover_position(&self, rover: RoverId) -> Option<Vec2> {
        self.context
            .borrow()
            .world()
            .rover(rover)
            .map(|r| r.position())
    }

    /// Number of commands queued for a rover.
    pub fn rover_queue_len(&self, rover: RoverId) -> Option<usize> {
        self.context
            .borrow()
            .world()
            .rover(rover)
            .map(|r| r.commands().len())
    }

    /// Take all log entries (read → cleared), oldest first.
    pub fn drain_logs(&mut self) -> Vec<LogEntry> {
        self.context.borrow_mut().logs_mut().drain()
    }

    /// Read-only view of the current log entries.
    pub fn log_snapshot(&self) -> Vec<LogEntry> {
        self.context.borrow().logs().snapshot()
    }
}

impl ScriptHost {
    /// Test/bridge helper: state hash of the world (golden comparisons).
    pub fn world_state_hash(&self) -> u64 {
        neogen_core::state_hash(self.context.borrow().world().state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evals_arithmetic() {
        let runtime = Runtime::new(RuntimeConfig::default()).expect("runtime creates");
        assert_eq!(
            runtime.eval("return 1 + 1").expect("eval"),
            EvalValue::Number(2.0)
        );
    }

    #[test]
    fn syntax_error_is_reported() {
        let runtime = Runtime::new(RuntimeConfig::default()).expect("runtime creates");
        assert!(runtime.eval("return +").is_err());
    }
}
