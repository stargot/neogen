//! World state, entity registry, and the [`World`] driver.

use std::collections::BTreeMap;
use std::collections::VecDeque;

use crate::commands::{Command, CommandError, ScanResult};
use crate::generate::generate_world;
use crate::ids::{IdIssuer, RoverId};
use crate::math::Vec2;
use crate::rng::Rng;
use crate::rover::step_rover;

/// Failure of a `push_commands` batch (FIX-раунд 1.5 #3/#8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushCommandsError {
    /// No rover with this id. The bridge must surface this to the script
    /// as a visible error — silently dropping commands hides dead rovers
    /// from the player.
    UnknownRover,
    /// The command at `index` (0-based, in batch order) failed validation;
    /// the whole batch was rejected.
    Invalid { index: usize, error: CommandError },
}

impl std::fmt::Display for PushCommandsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRover => write!(f, "no rover with this id"),
            Self::Invalid { index, error } => {
                write!(f, "command #{index} invalid: {error}")
            }
        }
    }
}

impl std::error::Error for PushCommandsError {}

/// A rover entity.
///
/// Physical state (position, heading, speed) plus the command queue and
/// scan buffer. Movement is command-driven (backlog 1.5): a rover without
/// queued commands parks, regardless of its speed field.
#[derive(Debug, Clone, PartialEq)]
pub struct Rover {
    /// Entity id (registry key).
    pub id: RoverId,
    /// Position in world units.
    pub position: Vec2,
    /// Heading as a (approximately unit) direction vector, computed with
    /// IEEE basic operations only (`delta / distance`) — never via
    /// `atan2`/`sin`/`cos`, whose libm last bit varies between platforms
    /// (FIX-раунд 1.5 #2). Updated while moving.
    pub heading: Vec2,
    /// Cruise-speed override in world units per tick. Must be finite and
    /// `>= 0.0`; `0.0` (the spawn default) selects the factory default
    /// ([`crate::rover::DEFAULT_CRUISE_SPEED`]). A negative value is
    /// invalid input: direct field writes would silently fall back to the
    /// factory default (any value `<= 0` does); the sanctioned write path
    /// (set-speed API) rejects it (FIX-раунд 1.5 #6).
    pub speed: f64,
    /// Queued commands, front executes first. *Inputs*, not physics:
    /// excluded from the state hash and from snapshots (see `hash.rs`).
    pub commands: VecDeque<Command>,
    /// Completed scans, oldest first. *Derived data*: excluded from the
    /// state hash and from snapshots (see `hash.rs`).
    pub scan_buffer: Vec<ScanResult>,
    /// Progress of the in-progress command: ticks left for the current
    /// Scan (crate-internal; reset when any command completes).
    pub(crate) remaining_ticks: u64,
}

impl Rover {
    /// Advance the front command by one tick (crate-internal: `step`
    /// drives this in ascending-id order).
    fn advance(&mut self, completing_tick: u64) {
        step_rover(self, completing_tick);
    }
}

/// Full deterministic state of the simulated world.
///
/// Everything needed to continue the simulation identically lives here
/// (tick, seed, entities, id counter), so a [`Clone`] of this struct is a
/// valid frozen snapshot — the foundation for the save system (backlog 1.4).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    /// Logical tick number; 0 for a freshly created world.
    pub tick: u64,
    /// Seed the world was generated from (used by backlog 1.2).
    pub seed: u64,
    /// Entity registry, keyed by id.
    ///
    /// Deliberately a `BTreeMap`, not a `HashMap`: iteration order must be
    /// deterministic (ascending id) both for stepping entities and for the
    /// future state hash (backlog 1.3). `HashMap`'s iteration order depends
    /// on `RandomState`, which would silently break determinism and golden
    /// tests on every run.
    rovers: BTreeMap<RoverId, Rover>,
    /// Sequential id issuer (see [`crate::ids`]).
    id_issuer: IdIssuer,
    /// World RNG, seeded from the seed at generation and advanced by every
    /// draw. Its position in the stream is part of the state (and of the
    /// future snapshot format, backlog 1.4) so that continuation after a
    /// save/load draws the same numbers as an uninterrupted run.
    rng: Rng,
}

impl WorldState {
    /// Empty world state for a seed: tick 0, fresh id issuer, RNG seeded
    /// from the seed. Used by [`crate::generate::generate_world`].
    pub(crate) fn empty(seed: u64) -> Self {
        Self {
            tick: 0,
            seed,
            rovers: BTreeMap::new(),
            id_issuer: IdIssuer::new(),
            rng: Rng::from_seed(seed),
        }
    }

    /// The world RNG (read-only view; see the field docs).
    pub fn rng(&self) -> &Rng {
        &self.rng
    }

    /// The world RNG for drawing (crate-internal: draws are part of the
    /// generation/simulation format and must stay ordered).
    pub(crate) fn rng_mut(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// Id-issuer watermark: the id the next spawned rover will receive
    /// (snapshot serialization; crate-internal).
    pub(crate) fn next_rover_id(&self) -> u32 {
        self.id_issuer.next_raw()
    }

    /// Rebuild a state from already-validated parts (snapshot decoding;
    /// crate-internal). `next_rover_id` must be above every rover id.
    pub(crate) fn from_parts(
        tick: u64,
        seed: u64,
        rovers: BTreeMap<RoverId, Rover>,
        next_rover_id: u32,
        rng: Rng,
    ) -> Self {
        Self {
            tick,
            seed,
            rovers,
            id_issuer: IdIssuer::resume(next_rover_id),
            rng,
        }
    }
    /// Spawn a rover with the given start pose and return its id.
    ///
    /// Ids come from the sequential issuer, so spawn order alone decides
    /// them — seed-independent and deterministic.
    pub fn spawn_rover(&mut self, position: Vec2, heading: Vec2) -> RoverId {
        let id = self.id_issuer.issue();
        let rover = Rover {
            id,
            position,
            heading,
            speed: 0.0,
            commands: VecDeque::new(),
            scan_buffer: Vec::new(),
            remaining_ticks: 0,
        };
        self.rovers.insert(id, rover);
        id
    }

    /// Append commands to a rover's queue (script entry point).
    ///
    /// Atomic all-or-nothing semantics (FIX-раунд 1.5 #3): the rover id is
    /// checked first ([`PushCommandsError::UnknownRover`]), then every
    /// command is validated; only if the *whole batch* is valid is anything
    /// queued. Returns the total queue length after the push.
    pub fn push_commands(
        &mut self,
        id: RoverId,
        commands: impl IntoIterator<Item = Command>,
    ) -> Result<usize, PushCommandsError> {
        if !self.rovers.contains_key(&id) {
            return Err(PushCommandsError::UnknownRover);
        }
        let batch: Vec<Command> = commands.into_iter().collect();
        for (index, command) in batch.iter().enumerate() {
            command
                .validate()
                .map_err(|error| PushCommandsError::Invalid { index, error })?;
        }
        let rover = self.rovers.get_mut(&id).expect("existence checked above");
        for command in batch {
            rover.commands.push_back(command);
        }
        Ok(rover.commands.len())
    }

    /// Iterate rovers in ascending-id order.
    pub fn rovers(&self) -> impl Iterator<Item = &Rover> {
        self.rovers.values()
    }

    /// Look up a rover by id.
    pub fn rover(&self, id: RoverId) -> Option<&Rover> {
        self.rovers.get(&id)
    }

    /// Look up a rover by id for mutation (used by commands in 1.5).
    pub fn rover_mut(&mut self, id: RoverId) -> Option<&mut Rover> {
        self.rovers.get_mut(&id)
    }
}

/// Owner and driver of a [`WorldState`].
///
/// `World` is the simulation entry point: create from a seed, then call
/// [`World::step`] once per logical tick. The struct is cheap to clone and
/// two clones stepped identically stay identical (determinism gate).
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    state: WorldState,
}

impl World {
    /// Create a fresh world from a seed via the deterministic generator
    /// ([`crate::generate::generate_world`]): one parked rover at a seeded
    /// position on the starting pad.
    pub fn new(seed: u64) -> Self {
        Self::from_state(generate_world(seed))
    }

    /// Resume a world from a state snapshot (continuation of a clone).
    pub fn from_state(state: WorldState) -> Self {
        Self { state }
    }

    /// Consume the world and hand over its state (for tests and snapshots).
    pub fn into_state(self) -> WorldState {
        self.state
    }

    /// Advance the simulation by exactly one fixed tick: rovers execute
    /// their command queues in ascending-id order, then the tick counter
    /// increments (entities first — unchanged since backlog 1.1).
    pub fn step(&mut self) {
        // Tick number this step completes — used to timestamp results.
        let completing_tick = self.state.tick.saturating_add(1);
        for rover in self.state.rovers.values_mut() {
            rover.advance(completing_tick);
        }
        self.state.tick = self.state.tick.saturating_add(1);
    }

    /// Current immutable state (a clone of it is a snapshot).
    pub fn state(&self) -> &WorldState {
        &self.state
    }

    /// Current logical tick number.
    pub fn tick(&self) -> u64 {
        self.state.tick
    }

    /// Seed this world was created from.
    pub fn seed(&self) -> u64 {
        self.state.seed
    }

    /// Iterate rovers in ascending-id order.
    pub fn rovers(&self) -> impl Iterator<Item = &Rover> {
        self.state.rovers()
    }

    /// Look up a rover by id.
    pub fn rover(&self, id: RoverId) -> Option<&Rover> {
        self.state.rover(id)
    }

    /// Append commands to a rover's queue (delegates to the state).
    pub fn push_commands(
        &mut self,
        id: RoverId,
        commands: impl IntoIterator<Item = Command>,
    ) -> Result<usize, PushCommandsError> {
        self.state.push_commands(id, commands)
    }

    /// Look up a rover by id for mutation (e.g. setting speed in tests,
    /// command application in backlog 1.5).
    pub fn rover_mut(&mut self, id: RoverId) -> Option<&mut Rover> {
        self.state.rover_mut(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandError;
    use crate::math::Vec2;

    #[test]
    fn fresh_world_starts_at_tick_zero_with_one_parked_rover() {
        let world = World::new(7);
        assert_eq!(world.tick(), 0);
        assert_eq!(world.seed(), 7);
        let rovers: Vec<_> = world.rovers().collect();
        assert_eq!(rovers.len(), 1);
        let rover = rovers[0];
        // Parked on the starting pad (position itself is seeded — 1.2).
        let bound = crate::generate::START_PAD_HALF_SIZE;
        assert!((-bound..bound).contains(&rover.position.x));
        assert!((-bound..bound).contains(&rover.position.y));
        assert_eq!(rover.speed, 0.0);
        // Heading is a normalized (approximately unit) direction vector.
        let length = rover.heading.length();
        assert!((length - 1.0).abs() < 1e-12, "not unit: {length}");
    }

    #[test]
    fn parked_rover_stays_put() {
        let mut world = World::new(1);
        let start = world.rovers().next().expect("rover exists").position;
        for _ in 0..10 {
            world.step();
        }
        let rover = world.rovers().next().expect("rover exists");
        assert_eq!(rover.position, start);
        assert!(rover.commands.is_empty());
    }

    #[test]
    fn invalid_batch_is_rejected_atomically() {
        let mut world = World::new(3);
        let id = world.rovers().next().expect("rover exists").id;
        let error = world
            .push_commands(
                id,
                [
                    Command::MoveTo {
                        target: Vec2::new(1.0, 1.0),
                    },
                    Command::Scan { radius: -1.0 },
                ],
            )
            .unwrap_err();
        assert_eq!(
            error,
            PushCommandsError::Invalid {
                index: 1,
                error: CommandError::NegativeRadius
            }
        );
        // Nothing was queued — the whole batch was rejected.
        assert!(world.rover(id).expect("rover exists").commands.is_empty());
    }

    #[test]
    fn push_to_unknown_rover_is_an_error() {
        let mut world = World::new(3);
        let ghost = RoverId::from_raw(999);
        assert_eq!(
            world.push_commands(ghost, [Command::Noop]).unwrap_err(),
            PushCommandsError::UnknownRover
        );
        // Unknown rover is checked even before command validation.
        assert_eq!(
            world
                .push_commands(ghost, [Command::Scan { radius: -1.0 }])
                .unwrap_err(),
            PushCommandsError::UnknownRover
        );
    }

    #[test]
    fn push_error_messages_are_readable() {
        let e = PushCommandsError::Invalid {
            index: 1,
            error: CommandError::NotFinite { field: "target.x" },
        }
        .to_string();
        assert!(e.contains("#1") && e.contains("target.x"), "{e}");
        let e = PushCommandsError::UnknownRover.to_string();
        assert!(e.contains("rover"), "{e}");
    }
}
