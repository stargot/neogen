//! Deterministic simulation core of Neogen.
//!
//! `neogen-core` owns the fixed-timestep world simulation: world state,
//! seed-based deterministic generation, rovers and command queues, and
//! snapshots. The crate is a deliberate dependency-free island — it must not
//! depend on Godot, GDExtension bindings, or any third-party crate, so the
//! simulation stays deterministic, portable, and testable headless.

pub mod generate;
pub mod hash;
pub mod ids;
pub mod math;
pub mod rng;
pub mod tick;
pub mod world;

pub use generate::{START_PAD_HALF_SIZE, generate_world};
pub use hash::{Fnv1a, state_hash};
pub use ids::{IdIssuer, RoverId};
pub use math::Vec2;
pub use rng::Rng;
pub use tick::{TICK_DT, TICK_HZ};
pub use world::{Rover, World, WorldState};

/// Version of the simulation engine API (matches the crate version).
pub fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
