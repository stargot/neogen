//! World state, entity registry, and the [`World`] driver.

use std::collections::BTreeMap;

use crate::generate::generate_world;
use crate::ids::{IdIssuer, RoverId};
use crate::math::Vec2;
use crate::rng::Rng;

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
        let rover = rovers[0];
        // Parked on the starting pad (position itself is seeded — 1.2).
        let bound = crate::generate::START_PAD_HALF_SIZE;
        assert!((-bound..bound).contains(&rover.position.x));
        assert!((-bound..bound).contains(&rover.position.y));
        assert_eq!(rover.speed, 0.0);
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
    }
}
