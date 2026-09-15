//! Player-script commands as pure data (backlog 1.5).
//!
//! Commands carry no behavior — execution lives in [`crate::rover`]. The
//! enum is the data contract between the future Lua runtime (phase 2) and
//! the simulation core.

use crate::math::Vec2;

/// Duration of a [`Command::Scan`], in ticks.
pub const SCAN_TICKS: u64 = 5;

/// One command for a rover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// Move to `target` at the rover's cruise speed; completes when the
    /// rover lands exactly on the target.
    MoveTo {
        /// Target point in world units.
        target: Vec2,
    },
    /// Scan the surroundings within `radius`; takes [`SCAN_TICKS`] ticks
    /// and appends a [`ScanResult`] to the rover's scan buffer.
    Scan {
        /// Scan radius in world units.
        radius: f64,
    },
    /// Placeholder for the `act` command: terraforming actions arrive in
    /// phase 8, so for now `act` completes in one tick and does nothing.
    /// Keeping the variant now pins the command wire format early.
    Noop,
}

/// Result of a completed [`Command::Scan`], stored in the rover's scan
/// buffer (oldest first).
///
/// `points` is a placeholder empty list until the world has tiles to scan
/// (phase 7); the structure — and its determinism — is the contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
    /// World tick at which the scan completed.
    pub tick: u64,
    /// Radius as requested by the command.
    pub radius: f64,
    /// Points discovered; empty placeholder until phase 7.
    pub points: Vec<Vec2>,
}
