//! `SimNode` — the Godot-facing simulation driver (backlog 3.2).
//!
//! Ownership: the node owns a [`ScriptHost`], which in turn owns the
//! `World` (through its shared `WorldContext`, the pattern established in
//! neogen-script 2.4). `SimNode` talks to the world exclusively through
//! the host's accessors — the same path scripts use — so there is exactly
//! one owner and one borrowing discipline (single-threaded, main thread,
//! as GDExtension callbacks are).

use godot::classes::INode;
use godot::classes::Node;
use godot::prelude::*;

use neogen_core::{Command, RoverId, TICK_DT, Vec2};
use neogen_script::{RuntimeConfig, ScriptHost, ScriptState};

use crate::coords;

/// Drives the simulation from Godot's physics loop.
///
/// The tick is decoupled from `_physics_process` frequency: delta time
/// accumulates, and for every full [`TICK_DT`] (1/30 s, core constant)
/// exactly one `world.step()` runs. Same wall time — same tick count, on
/// any machine; the simulation itself never sees wall-clock deltas.
#[derive(GodotClass)]
#[class(base = Node)]
pub(crate) struct SimNode {
    base: Base<Node>,
    /// Simulation seed. Applied once when the simulation host is created
    /// (first use / scene start) — **changing it afterwards has no
    /// effect** on the already-running world.
    #[var]
    seed: i64,
    host: Option<ScriptHost>,
    /// Latched failure of host creation (review #4): no per-call retries,
    /// exactly one error is logged.
    host_failed: bool,
    accumulator: f64,
}

#[godot_api]
impl INode for SimNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            seed: 42,
            host: None,
            host_failed: false,
            accumulator: 0.0,
        }
    }

    fn ready(&mut self) {
        self.ensure_host();
    }

    fn physics_process(&mut self, delta: f64) {
        self.accumulator += delta;
        while self.accumulator >= TICK_DT {
            self.step_and_emit();
            self.accumulator -= TICK_DT;
        }
    }
}

impl SimNode {
    /// Create the simulation host on first use. `ready()` is the normal
    /// path for scene-embedded nodes; scripts that instantiate the node
    /// during `SceneTree._initialize` run before the tree delivers
    /// notifications, so every accessor goes through here too (lazy init).
    fn ensure_host(&mut self) -> Option<&mut ScriptHost> {
        if self.host_failed {
            return None; // latched: one error, no per-call retries
        }
        if self.host.is_none() {
            match ScriptHost::new(
                neogen_core::World::new(self.seed as u64),
                RuntimeConfig::default(),
            ) {
                Ok(host) => self.host = Some(host),
                Err(error) => {
                    self.host_failed = true;
                    godot_error!("Neogen: cannot create simulation host: {error}");
                    return None;
                }
            }
        }
        self.host.as_mut()
    }
}

#[godot_api]
impl SimNode {
    /// Emitted for every script log line, one signal per entry, after the
    /// tick that produced it (backlog 3.4). Handlers must not synchronously
    /// call back into `step_ticks`/`step_world` (nested ticks); defer
    /// heavy reactions to the next frame instead.
    #[signal]
    fn log_line(tick: i64, rover_id: i64, text: GString);

    /// Current simulation tick.
    #[func]
    fn get_tick(&mut self) -> i64 {
        self.ensure_host().map_or(0, |host| host.world_tick()) as i64
    }

    /// Fraction of the way to the next tick, 0..1 (render interpolation
    /// input: how close the accumulator is to the next `TICK_DT`).
    #[func]
    pub(crate) fn tick_alpha(&self) -> f32 {
        (self.accumulator / TICK_DT).clamp(0.0, 1.0) as f32
    }

    /// Ids of all managed scripts, ascending (HUD, backlog 4.3).
    #[func]
    fn get_script_ids(&mut self) -> PackedInt64Array {
        let mut ids = PackedInt64Array::new();
        if let Some(host) = self.ensure_host() {
            for id in host.managed_ids() {
                ids.push(id as i64);
            }
        }
        ids
    }

    /// Lifecycle state of a managed script as `{state: String, error:
    /// String}` — `state` is one of running/suspended/finished/stopped/
    /// error/unknown; `error` carries the message only in the error case.
    /// Minimal bridge adaptation for the HUD (backlog 4.3).
    #[func]
    fn get_script_state(&mut self, id: i64) -> VarDictionary {
        let mut out = VarDictionary::new();
        let state = self
            .ensure_host()
            .and_then(|host| host.script_state(id as u32));
        match state {
            None => {
                out.set("state", "unknown");
            }
            Some(ScriptState::Running) => {
                out.set("state", "running");
            }
            Some(ScriptState::SuspendedBudget) => {
                out.set("state", "suspended");
            }
            Some(ScriptState::Finished) => {
                out.set("state", "finished");
            }
            Some(ScriptState::Stopped) => {
                out.set("state", "stopped");
            }
            Some(ScriptState::Failed(error)) => {
                out.set("state", "error");
                out.set("error", error.to_string());
            }
        }
        out
    }

    /// Rover's effective speed: cruise speed while driving, 0 when
    /// parked (backlog 4.2). -1 for unknown rovers.
    #[func]
    fn get_rover_speed(&mut self, id: i64) -> f64 {
        self.ensure_host()
            .and_then(|host| host.rover_speed(RoverId::from_raw(id as u32)))
            .unwrap_or(-1.0)
    }

    /// Rover's heading in Godot coordinates (approximately unit vector;
    /// ZERO for unknown rovers).
    #[func]
    fn get_rover_heading(&mut self, id: i64) -> Vector2 {
        self.ensure_host()
            .and_then(|host| host.rover_heading(RoverId::from_raw(id as u32)))
            .map_or(Vector2::ZERO, coords::to_godot)
    }

    /// Rover ids present in the world, ascending.
    #[func]
    pub(crate) fn get_rover_ids(&mut self) -> PackedInt64Array {
        let mut ids = PackedInt64Array::new();
        if let Some(host) = self.ensure_host() {
            for id in host.rover_ids() {
                ids.push(id.raw() as i64);
            }
        }
        ids
    }

    /// Rover position in Godot coordinates (see `coords` for the Y flip).
    #[func]
    pub(crate) fn get_rover_position(&mut self, id: i64) -> Vector2 {
        let position = self
            .ensure_host()
            .and_then(|host| host.rover_position(RoverId::from_raw(id as u32)));
        match position {
            Some(position) => coords::to_godot(position),
            None => {
                godot_warn!("Neogen: no rover with id {id}");
                Vector2::ZERO
            }
        }
    }

    /// Advance exactly `n` ticks right now. **Test/deterministic path**
    /// (review #5/#8): the real game loop is the physics accumulator;
    /// this bypasses it but keeps the world/scheduler semantics.
    #[func]
    fn step_ticks(&mut self, n: i64) {
        for _ in 0..n.max(0) {
            self.step_and_emit();
        }
    }

    /// Attach a Lua script to a rover (managed: runs inside every
    /// `step_world`). Returns the script id, or **-1 on any error**
    /// (unknown rover id, compile failure) — nothing is attached then.
    #[func]
    fn attach_script(&mut self, rover_id: i64, source: GString) -> i64 {
        let Some(host) = self.ensure_host() else {
            return -1;
        };
        let rover = RoverId::from_raw(rover_id as u32);
        if !host.rover_ids().contains(&rover) {
            godot_warn!("Neogen: attach_script: no rover with id {rover_id}");
            return -1;
        }
        match host.attach_script(rover, &source.to_string()) {
            Ok(id) => id as i64,
            Err(error) => {
                godot_warn!("Neogen: attach_script failed: {error}");
                -1
            }
        }
    }

    /// Queue a move for a rover (test/diagnostic channel; player scripts
    /// use their own API). **Coordinates are core-world units** (Y north),
    /// mirroring the Lua `move(x, y)` API — convert user-facing screen
    /// input with the inverse of `get_rover_position` (see `coords`).
    /// Returns false for unknown rovers or invalid targets.
    #[func]
    fn debug_move_rover(&mut self, id: i64, x: f64, y: f64) -> bool {
        let Some(host) = self.host.as_mut() else {
            return false;
        };
        let command = Command::MoveTo {
            target: Vec2::new(x, y),
        };
        match host.push_command(RoverId::from_raw(id as u32), command) {
            Ok(()) => true,
            Err(error) => {
                godot_warn!("Neogen: debug_move_rover failed: {error}");
                false
            }
        }
    }
}

impl SimNode {
    /// One world tick, then drain the log buffer and emit one
    /// `log_line` signal per entry. Draining is read → cleared
    /// (`ScriptHost::drain_logs`), so repeat polls never duplicate; ring
    /// overflow already dropped the oldest entries inside the buffer.
    fn step_and_emit(&mut self) {
        if let Some(host) = self.host.as_mut() {
            host.step_world();
        }
        let entries = match self.host.as_mut() {
            Some(host) => host.drain_logs(),
            None => Vec::new(),
        };
        for entry in entries {
            self.signals().log_line().emit(
                entry.tick as i64,
                entry.rover_id.raw() as i64,
                &GString::from(&entry.text),
            );
        }
    }
}
