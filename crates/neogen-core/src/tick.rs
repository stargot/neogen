//! Fixed-timestep mechanics of the simulation.
//!
//! The world never advances by wall-clock deltas: every logical change
//! happens in discrete [`World::step`](crate::World::step) calls, each worth
//! exactly [`TICK_DT`] seconds. Hosts (the Godot bridge in `neogen-gdext`)
//! accumulate real time and issue whole steps — never partial ones — so the
//! simulation is reproducible from a seed alone.

/// Simulation frequency: 30 logical ticks per second.
///
/// Chosen over 60 Hz because the core simulates engineering-scale rover
/// movement and slow terraforming, not twitch physics: 30 Hz halves the work
/// per simulated second while staying smooth after render-side interpolation
/// (backlog phase 3). Changing this constant changes the meaning of saved
/// worlds — treat it as part of the snapshot format.
pub const TICK_HZ: u32 = 30;

/// Duration of one logical tick, in seconds: exactly `1.0 / TICK_HZ`.
pub const TICK_DT: f64 = 1.0 / TICK_HZ as f64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_dt_matches_frequency() {
        assert_eq!(TICK_DT, 1.0 / f64::from(TICK_HZ));
        assert_eq!(f64::from(TICK_HZ) * TICK_DT, 1.0);
    }
}
