//! Lua runtime for player scripts (backlog phase 2).
//!
//! `neogen-script` owns the Lua 5.4 runtime (via `mlua`, vendored — no
//! system Lua) and hosts the script sandbox (2.2), the per-tick instruction
//! budget (2.3), the `move/scan/act/print` API (2.4), script error
//! handling and lifecycle (2.5) and the multi-script scheduler (2.6).
//! Like `neogen-core`, this crate never depends on Godot or GDExtension
//! bindings: the simulation stays headless-testable, the bridge consumes
//! it from `neogen-gdext`.
//!
//! The runtime is deliberately **synchronous** (no mlua `async` feature):
//! scripts advance in discrete deterministic ticks, not on wall-clock I/O.

mod api;
mod budget;
mod errors;
mod lifecycle;
mod logbuffer;
mod sandbox;
mod scheduler;
mod script_store;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Lua, LuaOptions};
use neogen_core::{RoverId, Vec2};

pub use api::{ACT_KINDS, ScriptContext, WorldContext};
pub use budget::{
    DEFAULT_HOOK_INTERVAL, DEFAULT_INSTRUCTIONS_PER_TICK, DEFAULT_MEMORY_LIMIT, RuntimeConfig,
    Script, TickOutcome,
};
pub use errors::{MAX_ERROR_TEXT, ScriptError};
pub use lifecycle::ScriptState;
pub use logbuffer::{LogBuffer, LogEntry, MAX_LOG_BUFFER};
pub use script_store::{FailedFile, ScanReport, ScriptStore, StoreError, StoredScript};

/// Library tables that each script env receives as its own copy (see
/// `ScriptHost::build_script`).
const LIB_TABLES: &[&str] = &["math", "string", "table", "coroutine", "utf8"];

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
        let lua = Lua::new_with(sandbox::safe_libs(), LuaOptions::default())
            .map_err(|error| ScriptError::runtime(0, 0, &error))?;
        sandbox::install(&lua)?;
        if config.memory_limit > 0 {
            lua.set_memory_limit(config.memory_limit)
                .map_err(|error| ScriptError::runtime(0, 0, &error))?;
        }
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

    /// Evaluate a chunk and return its `return` value.
    ///
    /// The raw escape hatch (tests, REPL-style experiments): runs on the
    /// shared sandboxed globals with no script identity — errors carry
    /// `script_id = 0` (ids start at 1). Player scripts run through
    /// [`ScriptHost`] instead.
    pub fn eval(&self, chunk: &str) -> Result<EvalValue, ScriptError> {
        let value = self
            .lua
            .load(chunk)
            .eval::<mlua::Value>()
            .map_err(|error| match &error {
                mlua::Error::SyntaxError { .. } => ScriptError::compile(&error),
                _ => ScriptError::runtime(0, 0, &error),
            })?;
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
///
/// Two driving modes:
/// - **Managed scripts** ([`attach_script`]) advance inside
///   [`ScriptHost::step_world`] — this is the player-facing path the
///   scheduler (2.6) formalizes. A failing script stops only itself
///   ([`ScriptState::Failed`]); every other script and the world tick on.
/// - **Manual scripts** ([`create_script`]) return the [`Script`] handle
///   for the caller to resume — tests and bridge-side custom driving
///   until 2.6 unifies the two.
pub struct ScriptHost {
    runtime: Runtime,
    context: api::SharedContext,
    managed: BTreeMap<u32, scheduler::ManagedScript>,
}

impl ScriptHost {
    /// Create a host over a world with the given runtime config.
    pub fn new(world: neogen_core::World, config: RuntimeConfig) -> Result<Self, ScriptError> {
        Ok(Self {
            runtime: Runtime::new(config)?,
            context: Rc::new(RefCell::new(WorldContext::new(world))),
            managed: BTreeMap::new(),
        })
    }

    /// Compile a script bound to a rover and hand the handle to the caller
    /// (manual driving; see the struct docs).
    pub fn create_script(&mut self, rover: RoverId, source: &str) -> Result<Script, ScriptError> {
        self.runtime.next_script_id = self.runtime.next_script_id.saturating_add(1);
        let id = self.runtime.next_script_id;
        self.build_script(rover, source, id)
    }

    /// Compile a script, register it in the tick loop and return its id.
    ///
    /// A compile error here changes nothing: the id is consumed, nothing
    /// is registered, the world is untouched.
    pub fn attach_script(&mut self, rover: RoverId, source: &str) -> Result<u32, ScriptError> {
        self.runtime.next_script_id = self.runtime.next_script_id.saturating_add(1);
        let id = self.runtime.next_script_id;
        let script = self.build_script(rover, source, id)?;
        self.managed.insert(
            id,
            scheduler::ManagedScript {
                script,
                rover,
                source: source.to_string(),
                state: ScriptState::Running,
            },
        );
        Ok(id)
    }

    /// Convenience bridge: attach a script from a [`ScriptStore`] entry by
    /// file name (backlog 2.7). A missing name is a host-side error
    /// (`script_id = 0`, see the `Runtime` docs for the convention).
    pub fn attach_from_store(
        &mut self,
        rover: RoverId,
        store: &ScriptStore,
        name: &str,
    ) -> Result<u32, ScriptError> {
        let stored = store.get(name).ok_or_else(|| ScriptError::Runtime {
            script_id: 0,
            tick: 0,
            message: format!(
                "no script named {name:?} in the store (dir: {})",
                store.dir().display()
            ),
        })?;
        self.attach_script(rover, &stored.text)
    }

    /// Restart a managed script with new source: fresh chunk, coroutine
    /// and counters, same script id, same rover, fresh env isolation.
    ///
    /// **Restart is manual by design** (MVP policy — see [`lifecycle`]):
    /// automatic restart policies may arrive post-MVP. If the new source
    /// fails to compile, the script keeps its previous state and the
    /// error is returned; nothing else changes.
    pub fn restart_script(&mut self, id: u32, source: &str) -> Result<(), ScriptError> {
        let Some(old) = self.managed.get(&id) else {
            return Err(ScriptError::Runtime {
                script_id: id,
                tick: 0,
                message: format!("restart_script: no managed script with id {id}"),
            });
        };
        let rover = old.rover;
        let replacement = self.build_script(rover, source, id)?;
        if let Some(managed) = self.managed.get_mut(&id) {
            managed.script = replacement;
            managed.source = source.to_string();
            managed.state = ScriptState::Running;
        }
        Ok(())
    }

    /// Lifecycle state of a managed script.
    pub fn script_state(&self, id: u32) -> Option<ScriptState> {
        self.managed.get(&id).map(|m| m.state.clone())
    }

    /// Ids of all managed scripts, ascending.
    pub fn managed_ids(&self) -> Vec<u32> {
        self.managed.keys().copied().collect()
    }

    /// Build a budgeted coroutine with a private env and the script API.
    fn build_script(
        &mut self,
        rover: RoverId,
        source: &str,
        id: u32,
    ) -> Result<Script, ScriptError> {
        let lua = &self.runtime.lua;

        // Private environment: copy of the sandboxed globals (minus the
        // excluded keys), plus the per-script API.
        let env = lua
            .create_table()
            .map_err(|error| ScriptError::runtime(id, 0, &error))?;
        let globals = lua.globals();
        let mut copied = 0usize;
        globals
            .for_each(|key: mlua::LuaString, value: mlua::Value| {
                let name = key.to_string_lossy();
                if name == "_G" || name == "_VERSION" {
                    return Ok(());
                }
                if let mlua::Value::Table(table) = &value
                    && LIB_TABLES.contains(&name.as_ref())
                {
                    // Per-script copy of a library table (FIX-раунд
                    // фазы 2 #4): a script poisoning `string.format`
                    // must not affect its neighbours. Functions are
                    // shared by value-reference — tables are the only
                    // mutable surface.
                    let copy = lua.create_table()?;
                    table.for_each(|k: mlua::Value, v: mlua::Value| copy.raw_set(k, v))?;
                    env.set(key, copy)?;
                    copied += 1;
                    return Ok(());
                }
                env.set(key, value)?;
                copied += 1;
                Ok(())
            })
            .map_err(|error| ScriptError::runtime(id, 0, &error))?;
        if copied == 0 {
            return Err(ScriptError::Runtime {
                script_id: id,
                tick: 0,
                message: "sandboxed globals are empty; refusing to build a script env".into(),
            });
        }

        let context: Rc<RefCell<dyn ScriptContext>> = Rc::clone(&self.context) as _;
        api::install(lua, &env, context, rover, id)?;

        let function = lua
            .load(source)
            .set_environment(env)
            .into_function()
            .map_err(|error| ScriptError::compile(&error))?;
        Script::spawn_function(lua, self.runtime.config, id, function)
    }

    /// Advance one simulation tick: resume every alive managed script in
    /// ascending-id order (deterministic), then step the world once.
    ///
    /// Failure policy: a script error moves *that* script to
    /// [`ScriptState::Failed`] and records an `error: …` line in the log
    /// buffer (world tick, script id, rover) — the tick loop, the world
    /// and all other scripts are unaffected. Budget exhaustion is a pause
    /// ([`ScriptState::SuspendedBudget`]), not a failure, and is not
    /// logged.
    pub fn step_world(&mut self) {
        scheduler::run_tick(&mut self.managed, &self.context);
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
        match runtime.eval("return +") {
            Err(ScriptError::Compile { .. }) => {}
            other => panic!("expected Compile, got {other:?}"),
        }
    }
}
