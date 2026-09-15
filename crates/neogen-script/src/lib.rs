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

mod budget;
mod sandbox;

use core::fmt;

use mlua::{Lua, LuaOptions};

pub use budget::{
    DEFAULT_HOOK_INTERVAL, DEFAULT_INSTRUCTIONS_PER_TICK, RuntimeConfig, Script, TickOutcome,
};

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
