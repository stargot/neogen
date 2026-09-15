//! Player-script commands as pure data (backlog 1.5).
//!
//! Commands carry no behavior — execution lives in [`crate::rover`]. The
//! enum is the data contract between the future Lua runtime (phase 2) and
//! the simulation core.

use core::fmt;

use crate::math::Vec2;

/// Duration of a [`Command::Scan`], in ticks.
pub const SCAN_TICKS: u64 = 5;

/// One command for a rover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// Move to `target` at the rover's cruise speed; completes when the
    /// rover lands exactly on the target.
    MoveTo {
        /// Target point in world units. Must be finite
        /// (see [`validate`](Command::validate)).
        target: Vec2,
    },
    /// Scan the surroundings within `radius`; takes [`SCAN_TICKS`] ticks
    /// and appends a [`ScanResult`] to the rover's scan buffer.
    Scan {
        /// Scan radius in world units; zero is allowed (a scan of
        /// nothing), negative is not.
        radius: f64,
    },
    /// Placeholder for the `act` command: terraforming actions arrive in
    /// phase 8, so for now `act` completes in one tick and does nothing.
    /// Keeping the variant now pins the command wire format early.
    Noop,
}

/// Validation failure of a single command (FIX-раунд 1.5 #3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    /// A component is NaN or infinite; `field` names it (`"target.x"`,
    /// `"radius"`, …).
    NotFinite { field: &'static str },
    /// Scan radius is negative.
    NegativeRadius,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite { field } => write!(f, "{field} is not finite"),
            Self::NegativeRadius => write!(f, "scan radius is negative"),
        }
    }
}

impl std::error::Error for CommandError {}

impl Command {
    /// Check the command's data for NaN/inf coordinates and negative
    /// radius. Invalid data must never enter the simulation state: it
    /// would poison positions, hashes and snapshots silently.
    pub fn validate(&self) -> Result<(), CommandError> {
        match self {
            Self::MoveTo { target } => {
                if !target.x.is_finite() {
                    Err(CommandError::NotFinite { field: "target.x" })
                } else if !target.y.is_finite() {
                    Err(CommandError::NotFinite { field: "target.y" })
                } else {
                    Ok(())
                }
            }
            Self::Scan { radius } => {
                if !radius.is_finite() {
                    Err(CommandError::NotFinite { field: "radius" })
                } else if *radius < 0.0 {
                    Err(CommandError::NegativeRadius)
                } else {
                    Ok(())
                }
            }
            Self::Noop => Ok(()),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_commands_pass() {
        assert_eq!(
            Command::MoveTo {
                target: Vec2::new(1.0, -2.5)
            }
            .validate(),
            Ok(())
        );
        assert_eq!(Command::Scan { radius: 0.0 }.validate(), Ok(()));
        assert_eq!(Command::Scan { radius: 7.0 }.validate(), Ok(()));
        assert_eq!(Command::Noop.validate(), Ok(()));
    }

    #[test]
    fn nan_and_inf_targets_are_rejected() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                Command::MoveTo {
                    target: Vec2::new(bad, 0.0)
                }
                .validate(),
                Err(CommandError::NotFinite { field: "target.x" })
            );
            assert_eq!(
                Command::MoveTo {
                    target: Vec2::new(0.0, bad)
                }
                .validate(),
                Err(CommandError::NotFinite { field: "target.y" })
            );
        }
    }

    #[test]
    fn bad_radius_is_rejected() {
        assert_eq!(
            Command::Scan { radius: -0.1 }.validate(),
            Err(CommandError::NegativeRadius)
        );
        assert_eq!(
            Command::Scan { radius: f64::NAN }.validate(),
            Err(CommandError::NotFinite { field: "radius" })
        );
        assert_eq!(
            Command::Scan {
                radius: f64::INFINITY
            }
            .validate(),
            Err(CommandError::NotFinite { field: "radius" })
        );
    }

    #[test]
    fn const_assert_scan_ticks_is_positive() {
        // Compile-time invariant (FIX-раунд 1.5 #7).
        const _: () = assert!(SCAN_TICKS >= 1);
    }
}
