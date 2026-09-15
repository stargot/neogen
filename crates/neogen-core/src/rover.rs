//! Rover command execution (backlog 1.5).
//!
//! Behavior for the command queue stored on [`Rover`] (the struct itself —
//! pure state — lives in [`crate::world`]). Execution is deterministic:
//! one tick advances at most the front command, MoveTo lands exactly on
//! its target without oscillation (`min(cruise, remaining distance)`), and
//! scan results are derived data appended to the rover's buffer in order.
//!
//! Heading determinism (FIX-раунд 1.5 #2): the heading is stored as a
//! direction vector computed only with IEEE 754 basic operations
//! (`delta / distance`, i.e. `÷` and `√`) — correctly rounded on every
//! compliant platform, so state hashes and snapshots match bit-for-bit
//! across Windows/Linux/macOS and any libm. Alternatives were rejected:
//! a quantized angle index needs trig (or big tables) for the conversion
//! back, and quantizing an `atan2` result still runs `atan2` — libm whose
//! last bit differs between C libraries.

use crate::commands::{Command, SCAN_TICKS, ScanResult};
use crate::math::Vec2;
use crate::world::Rover;

/// Factory cruise speed for MoveTo, in world units per tick.
///
/// Used when the rover's `speed` field is `0` (the spawn default); a
/// positive `speed` field overrides it, letting future scripts throttle
/// their rover. Keeping the spawn value at `0.0` also keeps the golden
/// hash fixtures stable — `speed` is part of the state walk.
pub const DEFAULT_CRUISE_SPEED: f64 = 2.0;

/// Maximum completed scans kept per rover (FIX-раунд 1.5 #4).
///
/// Overflow policy: oldest results are evicted — scripts care about the
/// latest scans, and a hard rejection would stall a script that forgets
/// to drain its buffer.
pub const MAX_SCAN_BUFFER: usize = 16;

impl Rover {
    /// Effective cruise speed for this tick (override or factory
    /// default). Any non-positive value — `0.0` by convention, a negative
    /// only via invalid input (see the `speed` field docs) — selects the
    /// default.
    pub fn cruise_speed(&self) -> f64 {
        if self.speed > 0.0 {
            self.speed
        } else {
            DEFAULT_CRUISE_SPEED
        }
    }

    /// Whether the rover is currently driving: the front command is a
    /// MoveTo that has not landed yet.
    pub fn is_moving(&self) -> bool {
        matches!(self.commands.front(), Some(Command::MoveTo { .. }))
    }

    /// Speed the rover actually covers ground with this tick (backlog
    /// 4.2, visual mirror): cruise speed while driving, 0 when parked.
    pub fn effective_speed(&self) -> f64 {
        if self.is_moving() {
            self.cruise_speed()
        } else {
            0.0
        }
    }
}

/// Advance the rover's front command by one tick. `tick` is the world tick
/// this step completes; it timestamps finished scan results.
pub(crate) fn step_rover(rover: &mut Rover, tick: u64) {
    let Some(command) = rover.commands.front().copied() else {
        return; // empty queue: the rover parks
    };
    let completed = match command {
        Command::MoveTo { target } => step_move_to(rover, target),
        Command::Scan { radius } => step_scan(rover, tick, radius),
        Command::Noop => true,
    };
    if completed {
        rover.commands.pop_front();
        rover.remaining_ticks = 0;
    }
}

/// One tick of movement toward `target`; returns `true` when arrived.
fn step_move_to(rover: &mut Rover, target: Vec2) -> bool {
    let delta = target - rover.position;
    let distance = delta.length();
    let speed = rover.cruise_speed();
    if distance <= speed {
        // Final tick: land exactly on the target — never overshoot, so no
        // oscillation around the goal.
        rover.position = target;
        true
    } else {
        // IEEE basic ops only (÷) — cross-platform bit-identical heading.
        rover.heading = delta / distance;
        rover.position = rover.position + delta / distance * speed;
        false
    }
}

/// One tick of scanning; returns `true` when the scan completes and its
/// result has been buffered.
fn step_scan(rover: &mut Rover, tick: u64, radius: f64) -> bool {
    if rover.remaining_ticks == 0 {
        // First tick of this scan: arm the duration.
        rover.remaining_ticks = SCAN_TICKS;
    }
    rover.remaining_ticks -= 1;
    if rover.remaining_ticks == 0 {
        rover.scan_buffer.push(ScanResult {
            tick,
            radius,
            points: Vec::new(),
        });
        // Cap the buffer: evict the oldest results (see MAX_SCAN_BUFFER).
        while rover.scan_buffer.len() > MAX_SCAN_BUFFER {
            rover.scan_buffer.remove(0);
        }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn rover_at(position: Vec2, speed: f64) -> Rover {
        Rover {
            id: crate::ids::RoverId::from_raw(1),
            position,
            heading: Vec2::new(1.0, 0.0),
            speed,
            commands: VecDeque::new(),
            scan_buffer: Vec::new(),
            remaining_ticks: 0,
        }
    }

    #[test]
    fn move_to_lands_exactly_in_non_round_ticks() {
        // Distance 5, cruise 2: ticks 1-2 move 2 each, tick 3 covers the
        // remaining 1 and lands exactly.
        let mut rover = rover_at(Vec2::ZERO, 2.0);
        let target = Vec2::new(3.0, 4.0);
        rover.commands.push_back(Command::MoveTo { target });

        step_rover(&mut rover, 1);
        assert!((rover.position.distance_to(target) - 3.0).abs() < 1e-12);
        step_rover(&mut rover, 2);
        assert!((rover.position.distance_to(target) - 1.0).abs() < 1e-12);
        step_rover(&mut rover, 3);
        assert_eq!(rover.position, target);
        assert!(rover.commands.is_empty());

        // No oscillation: further ticks do not move it.
        step_rover(&mut rover, 4);
        assert_eq!(rover.position, target);
    }

    #[test]
    fn move_to_at_target_completes_immediately() {
        let mut rover = rover_at(Vec2::new(1.0, 1.0), 0.0);
        rover.commands.push_back(Command::MoveTo {
            target: Vec2::new(1.0, 1.0),
        });
        step_rover(&mut rover, 1);
        assert!(rover.commands.is_empty());
    }

    #[test]
    fn zero_speed_field_means_default_cruise() {
        let mut rover = rover_at(Vec2::ZERO, 0.0);
        rover.commands.push_back(Command::MoveTo {
            target: Vec2::new(DEFAULT_CRUISE_SPEED, 0.0),
        });
        step_rover(&mut rover, 1);
        assert_eq!(rover.position, Vec2::new(DEFAULT_CRUISE_SPEED, 0.0));
    }

    #[test]
    fn scan_completes_after_scan_ticks() {
        let mut rover = rover_at(Vec2::ZERO, 0.0);
        rover.commands.push_back(Command::Scan { radius: 9.0 });
        for tick in 1..SCAN_TICKS {
            step_rover(&mut rover, tick);
            assert!(
                rover.scan_buffer.is_empty(),
                "completed too early at {tick}"
            );
        }
        step_rover(&mut rover, SCAN_TICKS);
        assert_eq!(rover.scan_buffer.len(), 1);
        let result = &rover.scan_buffer[0];
        assert_eq!(result.tick, SCAN_TICKS);
        assert_eq!(result.radius, 9.0);
        assert!(result.points.is_empty());
        assert!(rover.commands.is_empty());
    }
}

#[cfg(test)]
mod moving_tests {
    use super::*;
    use crate::world::World;
    use crate::{Command, RoverId};

    #[test]
    fn effective_speed_follows_the_command_queue() {
        let mut world = World::new(3);
        let id = world.rovers().next().expect("rover").id();
        let rover = world.rover(id).expect("rover");
        assert!(!rover.is_moving());
        assert_eq!(rover.effective_speed(), 0.0);

        world
            .push_commands(
                id,
                [Command::MoveTo {
                    target: Vec2::new(9.0, 9.0),
                }],
            )
            .expect("valid");
        let rover = world.rover(id).expect("rover");
        assert!(rover.is_moving());
        assert_eq!(rover.effective_speed(), DEFAULT_CRUISE_SPEED);
    }

    #[test]
    fn cruise_override_wins_while_moving() {
        let mut world = World::new(5);
        let id = world.rovers().next().expect("rover").id();
        world.set_rover_speed(id, 3.5).expect("valid");
        world
            .push_commands(
                id,
                [Command::MoveTo {
                    target: Vec2::new(9.0, 0.0),
                }],
            )
            .expect("valid");
        let rover = world.rover(id).expect("rover");
        assert_eq!(rover.cruise_speed(), 3.5);
        assert_eq!(rover.effective_speed(), 3.5);
    }

    #[test]
    fn unknown_rover_helpers_stay_out() {
        // The helpers live on Rover; there is no world-level lookup here.
        let _ = RoverId::from_raw(1);
    }
}
