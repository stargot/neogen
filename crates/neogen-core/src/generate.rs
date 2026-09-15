//! Seed-based generation of the starting world.
//!
//! For now the world is a single flat starting pad — a placeholder for the
//! rover to spawn on. Terrain, biomes and resources arrive in the world
//! phases (backlog 7.x), at which point this module grows the generation
//! pipeline but keeps the contract: `generate_world(seed)` is a pure
//! function of the seed.
//!
//! **Draw order is part of the world-generation format:** currently the RNG
//! draws rover `x`, `y`, direction `dx`, `dy` (each in the same fixed
//! order). Reordering or inserting draws changes every generated world for
//! the same seed — after snapshots land this counts as a format break.
//!
//! The spawn direction is drawn as a raw vector and normalized with
//! IEEE basic operations (`÷√`); no `sin`/`cos`/`atan2` is used, so the
//! generated state is bit-identical on any IEEE 754 platform regardless of
//! the C library (FIX-раунд 1.5 #2).

use crate::math::Vec2;
use crate::world::WorldState;

/// Half-size of the flat starting pad, in world units.
///
/// The pad itself is a conceptual placeholder (no terrain yet): the rover
/// spawns somewhere inside `[-START_PAD_HALF_SIZE, START_PAD_HALF_SIZE]²`,
/// with a seeded (approximately unit) direction.
pub const START_PAD_HALF_SIZE: f64 = 8.0;

/// Generate the starting world for a seed: one rover parked at a seeded
/// position on the flat pad, with a seeded direction and speed 0.
pub fn generate_world(seed: u64) -> WorldState {
    let mut state = WorldState::empty(seed);
    // Draw order (x, y, dx, dy) is part of the generation format — see
    // module docs.
    let x = state
        .rng_mut()
        .next_f64_in(-START_PAD_HALF_SIZE..START_PAD_HALF_SIZE);
    let y = state
        .rng_mut()
        .next_f64_in(-START_PAD_HALF_SIZE..START_PAD_HALF_SIZE);
    let dx = state.rng_mut().next_f64_in(-1.0..1.0);
    let dy = state.rng_mut().next_f64_in(-1.0..1.0);
    let heading = Vec2::new(dx, dy).normalized();
    state.spawn_rover(Vec2::new(x, y), heading);
    state
}
