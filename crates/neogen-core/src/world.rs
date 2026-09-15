//! World state, entity registry, and the [`World`] driver.

use std::collections::BTreeMap;

use crate::ids::{IdIssuer, RoverId};
use crate::math::Vec2;

/// A rover entity.
///
/// Movement is heading/speed based; command queues arrive in backlog 1.5.
/// `speed` is in world units per tick (not per second) so one `step` is
/// always `position += direction(heading) * speed`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rover {
    /// Entity id (registry key).
    pub id: RoverId,
    /// Position in world units.
    pub position: Vec2,
    /// Heading in radians (0 = +X, π/2 = +Y).
    pub heading: f64,
    /// Speed in world units per tick.
    pub speed: f64,
}

impl Rover {
    /// Advance the rover by exactly one tick along its heading.
    fn advance(&mut self) {
        let direction = Vec2::from_angle(self.heading);
        self.position = self.position + direction * self.speed;
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
}

impl WorldState {
    /// Spawn a rover with the given start pose and return its id.
    ///
    /// Ids come from the sequential issuer, so spawn order alone decides
    /// them — seed-independent and deterministic.
    pub fn spawn_rover(&mut self, position: Vec2, heading: f64) -> RoverId {
        let id = self.id_issuer.issue();
        let rover = Rover {
            id,
            position,
            heading,
            speed: 0.0,
        };
        self.rovers.insert(id, rover);
        id
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
    /// Create a fresh world from a seed: one rover parked at the origin,
    /// heading +X, speed 0.
    pub fn new(seed: u64) -> Self {
        let mut state = WorldState {
            tick: 0,
            seed,
            rovers: BTreeMap::new(),
            id_issuer: IdIssuer::new(),
        };
        state.spawn_rover(Vec2::ZERO, 0.0);
        Self { state }
    }

    /// Resume a world from a state snapshot (continuation of a clone).
    pub fn from_state(state: WorldState) -> Self {
        Self { state }
    }

    /// Consume the world and hand over its state (for tests and snapshots).
    pub fn into_state(self) -> WorldState {
        self.state
    }

    /// Advance the simulation by exactly one fixed tick
    /// (entities first, in ascending-id order, then the tick counter).
    pub fn step(&mut self) {
        for rover in self.state.rovers.values_mut() {
            rover.advance();
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

    /// Look up a rover by id for mutation (e.g. setting speed in tests,
    /// command application in backlog 1.5).
    pub fn rover_mut(&mut self, id: RoverId) -> Option<&mut Rover> {
        self.state.rover_mut(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_world_starts_at_tick_zero_with_one_parked_rover() {
        let world = World::new(7);
        assert_eq!(world.tick(), 0);
        assert_eq!(world.seed(), 7);
        let rovers: Vec<_> = world.rovers().collect();
        assert_eq!(rovers.len(), 1);
        assert_eq!(rovers[0].position, Vec2::ZERO);
        assert_eq!(rovers[0].speed, 0.0);
    }

    #[test]
    fn parked_rover_stays_at_origin() {
        let mut world = World::new(1);
        for _ in 0..10 {
            world.step();
        }
        let rover = world.rovers().next().expect("rover exists");
        assert_eq!(rover.position, Vec2::ZERO);
    }
}
