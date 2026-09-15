//! Script-facing world API: `move / scan / act / print` (backlog 2.4).
//!
//! # Contract
//!
//! - `move(x, y)` and `scan(radius)` **queue commands** for the script's
//!   rover through `neogen-core`'s `push_commands` — nothing executes
//!   instantly; the world applies queues on its own ticks. Core validation
//!   errors (non-finite coordinates, negative radius) surface as Lua
//!   runtime errors with the core's message, and the world stays
//!   unpoisoned (the core rejects batches atomically).
//! - `scan_result()` **peeks** the latest completed scan of the rover and
//!   returns `{ tick, radius, points }` (or `nil` before the first scan
//!   completes). Peek, not drain: scripts poll; draining stays with the
//!   bridge (phase 3.4) via the core's `take_scan_results`.
//! - `act(kind, params)` is the phase-8 stub: `kind` must be one of
//!   [`ACT_KINDS`] (`"plant"`, `"drain"`), `params` must be a table (or
//!   nil) and is **ignored until phase 8.1** — the command maps to
//!   `Command::Noop` for now, pinning the wire shape early.
//! - `print(...)` replaces the built-in: it renders arguments like Lua's
//!   own `print` and writes a [`LogEntry`] into the [`LogBuffer`] instead
//!   of stdout. The built-in `print` is removed from the globals (see
//!   `sandbox`).
//!
//! # Ownership / context decision
//!
//! Scripts never touch a `World` directly. Everything goes through the
//! [`ScriptContext`] trait (push command, peek scan, log, current tick),
//! implemented by [`WorldContext`] which owns the `World` and the
//! [`LogBuffer`]. A `ScriptHost` shares one `Rc<RefCell<WorldContext>>`
//! between itself and the per-script API closures — chosen over the
//! alternative (script crate borrowing the world) because Lua callbacks
//! fire *during* world ownership by the host; the `Rc<RefCell>` makes the
//! borrow discipline explicit and single-threaded by design (the runtime
//! is synchronous). Phase 2.6/3 will formalize the scheduler on top of
//! this; if the bridge ever needs its own context, it implements the same
//! trait.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};
use neogen_core::{Command, PushCommandsError, RoverId, ScanResult, Vec2, World};

use crate::ScriptError;
use crate::logbuffer::{LogBuffer, LogEntry};

/// Allowed `act` kinds (phase 8 will implement them; the list pins the
/// contract now).
pub const ACT_KINDS: &[&str] = &["plant", "drain"];

/// What scripts may do to the world. See the module docs for the contract.
pub trait ScriptContext {
    /// Queue one command for a rover (core validation applies).
    fn push_command(&mut self, rover: RoverId, command: Command) -> Result<(), PushCommandsError>;
    /// Latest completed scan of a rover, if any (peek — does not consume).
    fn latest_scan(&mut self, rover: RoverId) -> Option<ScanResult>;
    /// Record one log line.
    fn log(&mut self, entry: LogEntry);
    /// Current world tick (timestamps log lines).
    fn current_tick(&self) -> u64;
}

/// Concrete [`ScriptContext`]: owns the world and the log buffer.
#[derive(Debug)]
pub struct WorldContext {
    world: World,
    logs: LogBuffer,
}

impl WorldContext {
    /// Wrap a world (starting empty logs).
    pub fn new(world: World) -> Self {
        Self {
            world,
            logs: LogBuffer::new(),
        }
    }

    /// The wrapped world (read-only).
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable access to the world (host stepping).
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// The log buffer (read-only).
    pub fn logs(&self) -> &LogBuffer {
        &self.logs
    }

    /// The log buffer (mutable, for draining from the host).
    pub fn logs_mut(&mut self) -> &mut LogBuffer {
        &mut self.logs
    }
}

impl ScriptContext for WorldContext {
    fn push_command(&mut self, rover: RoverId, command: Command) -> Result<(), PushCommandsError> {
        self.world.push_commands(rover, [command]).map(|_| ())
    }

    fn latest_scan(&mut self, rover: RoverId) -> Option<ScanResult> {
        self.world
            .rover(rover)
            .and_then(|rover: &neogen_core::Rover| rover.scan_results().last().cloned())
    }

    fn log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
    }

    fn current_tick(&self) -> u64 {
        self.world.tick()
    }
}

/// Shared context handle used by the API closures and the host.
pub(crate) type SharedContext = Rc<RefCell<WorldContext>>;

/// Install the script API into a script's private environment table.
///
/// The functions close over the shared context, the script's rover id and
/// the script id — nothing script-visible is mutable from Rust afterwards.
pub(crate) fn install(
    lua: &Lua,
    env: &Table,
    context: Rc<RefCell<dyn ScriptContext>>,
    rover: RoverId,
    script_id: u32,
) -> Result<(), ScriptError> {
    let move_ctx = context.clone();
    env.set(
        "move",
        lua.create_function(move |_lua, (x, y): (f64, f64)| {
            let command = Command::MoveTo {
                target: Vec2::new(x, y),
            };
            push_validated(&move_ctx, rover, command)
        })?,
    )?;

    let scan_ctx = context.clone();
    env.set(
        "scan",
        lua.create_function(move |_lua, radius: f64| {
            let command = Command::Scan { radius };
            push_validated(&scan_ctx, rover, command)
        })?,
    )?;

    let peek_ctx = context.clone();
    env.set(
        "scan_result",
        lua.create_function(move |lua, (): ()| {
            let latest = peek_ctx.borrow_mut().latest_scan(rover);
            match latest {
                None => Ok(Value::Nil),
                Some(result) => scan_result_to_table(lua, &result),
            }
        })?,
    )?;

    let act_ctx = context.clone();
    env.set(
        "act",
        lua.create_function(move |_lua, (kind, params): (String, Option<Table>)| {
            if !ACT_KINDS.contains(&kind.as_str()) {
                return Err(mlua::Error::runtime(format!(
                    "act: unknown kind {kind:?} (allowed: {ACT_KINDS:?})"
                )));
            }
            // `params` is accepted (must be a table or nil) and ignored
            // until phase 8.1 — see the module docs.
            let _ = params;
            // Wire-shape stub: phase 8 maps kinds to real commands.
            push_validated(&act_ctx, rover, Command::Noop)
        })?,
    )?;

    let print_ctx = context;
    env.set(
        "print",
        lua.create_function(move |_lua, args: MultiValue| {
            let text = render_print_args(args);
            let mut ctx = print_ctx.borrow_mut();
            let tick = ctx.current_tick();
            ctx.log(LogEntry {
                tick,
                script_id,
                rover_id: rover,
                text,
            });
            Ok(())
        })?,
    )?;

    Ok(())
}

/// Validate a command and queue it, converting errors to Lua errors with
/// readable messages.
fn push_validated(
    context: &Rc<RefCell<dyn ScriptContext>>,
    rover: RoverId,
    command: Command,
) -> Result<(), mlua::Error> {
    if let Err(error) = command.validate() {
        return Err(mlua::Error::runtime(format!("{error}")));
    }
    context
        .borrow_mut()
        .push_command(rover, command)
        .map_err(|error| mlua::Error::runtime(format!("{error}")))
}

/// Render `print` arguments like Lua does: `tostring`, joined with tabs.
fn render_print_args(args: MultiValue) -> String {
    args.into_iter()
        .map(value_to_string)
        .collect::<Vec<_>>()
        .join("\t")
}

fn value_to_string(value: Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => format_f64(n),
        Value::String(s) => s.to_string_lossy(),
        other => format!("{:?}", other.type_name()),
    }
}

/// Format a float the way Lua's `tostring` does for common cases
/// (integral floats render without the fraction tail).
fn format_f64(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Build the `{ tick, radius, points }` table for `scan_result()`.
fn scan_result_to_table(lua: &Lua, result: &ScanResult) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;
    table.set("tick", result.tick)?;
    table.set("radius", result.radius)?;
    let points = lua.create_table()?;
    for (index, point) in result.points.iter().enumerate() {
        let point_table = lua.create_table()?;
        point_table.set("x", point.x)?;
        point_table.set("y", point.y)?;
        points.set(index + 1, point_table)?;
    }
    table.set("points", points)?;
    Ok(Value::Table(table))
}
