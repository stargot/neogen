//! State hashing for golden determinism tests (backlog 1.3).
//!
//! Own FNV-1a 64-bit hash over an explicit field walk of [`WorldState`] —
//! no external crates, no `derive(Hash)`.
//!
//! # Why an explicit walk
//!
//! Hashing must not depend on struct layout, padding or alignment: two
//! logically identical states on different platforms (or after a compiler
//! change) must produce the same hash. So the walk goes field by field in a
//! fixed order, and `f64`s are hashed through [`f64::to_bits`] — never as
//! raw struct bytes.
//!
//! # Hash stability contract
//!
//! The walk order below *is* the hash format:
//!
//! 1. `tick`
//! 2. `seed`
//! 3. the four RNG state words (order as stored)
//! 4. rover count
//! 5. per rover, in ascending-id order: id, position.x, position.y,
//!    heading.x, heading.y, speed (heading is a direction vector since
//!    FIX-раунд 1.5 #2 — a deliberate format change, fixtures regenerated)
//!
//! Any change to the state *shape* (new field, new entity kind, changed
//! entity semantics) intentionally changes the hashes — golden fixtures
//! will fail, which is exactly what they are for. When the change is
//! deliberate, regenerate the fixtures with `NEOGEN_UPDATE_GOLDEN=1`
//! (see `tests/golden_determinism.rs`) and mention the format change in
//! the commit.
//!
//! **Deliberately excluded from the walk:** rover command queues, scan
//! buffers, and the in-progress command progress (`remaining_ticks`).
//! Commands are *inputs* — two worlds that ended up in the same physical
//! state must hash the same regardless of which command sequence produced
//! it (and hash-drift tests must not fire just because a test queued
//! commands). Scan buffers and `remaining_ticks` are *derived data* —
//! replaying the same ticks with the same commands reconstructs them.
//! Nothing else that `step()` reads is missing: `step()` touches only the
//! tick counter and, per rover, the walked physical fields driven by the
//! (excluded) queues. The id-issuer watermark also stays out — it
//! influences future spawns, not `step()`. Snapshot serialization mirrors
//! these exclusions (see `snapshot.rs`).

use crate::world::WorldState;

/// FNV-1a offset basis (64-bit).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a prime (64-bit).
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a 64-bit hasher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fnv1a(u64);

impl Fnv1a {
    /// New hasher at the offset basis.
    pub const fn new() -> Self {
        Self(FNV_OFFSET_BASIS)
    }

    /// Mix in one byte.
    pub fn write_u8(&mut self, byte: u8) {
        self.0 ^= u64::from(byte);
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    /// Mix in one `u64`, least-significant byte first (fixed byte order —
    /// part of the hash format).
    pub fn write_u64(&mut self, word: u64) {
        for i in 0..8 {
            self.write_u8(((word >> (i * 8)) & 0xff) as u8);
        }
    }

    /// Mix in one `f64` through its bit pattern.
    pub fn write_f64(&mut self, value: f64) {
        self.write_u64(value.to_bits());
    }

    /// Finish and return the hash.
    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash the full world state (field walk — see module docs for the order).
pub fn state_hash(state: &WorldState) -> u64 {
    let mut h = Fnv1a::new();
    h.write_u64(state.tick);
    h.write_u64(state.seed);
    for word in state.rng().state_words() {
        h.write_u64(word);
    }
    h.write_u64(state.rovers().count() as u64);
    for rover in state.rovers() {
        h.write_u64(u64::from(rover.id.raw()));
        h.write_f64(rover.position.x);
        h.write_f64(rover.position.y);
        h.write_f64(rover.heading.x);
        h.write_f64(rover.heading.y);
        h.write_f64(rover.speed);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn identical_states_hash_equal() {
        let a = World::new(42);
        let b = World::new(42);
        assert_eq!(state_hash(a.state()), state_hash(b.state()));
    }

    #[test]
    fn tick_progress_changes_hash() {
        let mut world = World::new(42);
        let before = state_hash(world.state());
        world.step();
        assert_ne!(state_hash(world.state()), before);
    }

    #[test]
    fn rover_field_change_changes_hash() {
        let mut world = World::new(42);
        let id = world.rovers().next().expect("rover exists").id();
        let before = state_hash(world.state());
        world.set_rover_speed(id, 1.0).expect("valid speed");
        assert_ne!(state_hash(world.state()), before);
    }

    #[test]
    fn different_seeds_hash_different() {
        assert_ne!(
            state_hash(World::new(1).state()),
            state_hash(World::new(2).state())
        );
    }
}
