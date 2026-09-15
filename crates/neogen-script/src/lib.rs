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

mod sandbox;

use core::fmt;

use mlua::{Lua, LuaOptions};

/// Failure inside the Lua runtime (syntax error, runtime error, …).
///
/// Phase 2.5 extends this into the full script-lifecycle error set.
#[derive(Debug)]
pub enum Error {
    /// An error raised by Lua itself.
    Lua(mlua::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lua(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lua(error) => Some(error),
        }
    }
}

impl From<mlua::Error> for Error {
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
}

impl Runtime {
    /// Create a runtime with Lua 5.4 in the sandboxed environment (see
    /// [`sandbox`] for the policy): whitelist-constructed std libs, dangerous
    /// globals removed — all before any player code can run.
    ///
    /// Fallible on purpose: state creation and sandbox installation are
    /// part of the contract (2.1 reserved the `Result`).
    pub fn new() -> Result<Self, Error> {
        let lua = Lua::new_with(sandbox::safe_libs(), LuaOptions::default())?;
        sandbox::install(&lua)?;
        Ok(Self { lua })
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
    pub fn eval(&self, chunk: &str) -> Result<EvalValue, Error> {
        let value = self.lua.load(chunk).eval::<mlua::Value>()?;
        Ok(value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evals_arithmetic() {
        let runtime = Runtime::new().expect("runtime creates");
        assert_eq!(
            runtime.eval("return 1 + 1").expect("eval"),
            EvalValue::Number(2.0)
        );
    }

    #[test]
    fn syntax_error_is_reported() {
        let runtime = Runtime::new().expect("runtime creates");
        assert!(runtime.eval("return +").is_err());
    }
}
