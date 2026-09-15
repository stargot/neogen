//! Coordinate conversion between the simulation core and Godot.
//!
//! The core world is top-down with **Y pointing north** (math convention,
//! f64 world units). Godot 2D has **Y pointing down** (screen convention)
//! and uses f32. `to_godot`/`from_godot` are the single place where this
//! rule lives:
//!
//! - X passes through unchanged;
//! - **Y is flipped** (`-y`) so that "north" in the world reads as "up" on
//!   screen;
//! - f64 → f32 narrowing happens on the Godot side (rendering precision
//!   is enough; the authoritative state stays in the core).
//!
//! **API convention** (review #1): SimNode read accessors
//! (`get_rover_position`, …) return Godot/screen coordinates, while
//! command-style inputs (`debug_move_rover`, and the Lua `move(x, y)`
//! API) take **core coordinates** — mirroring the script API one-to-one;
//! use [`from_godot`] to convert user-facing screen input.

use godot::builtin::Vector2;

/// Convert a core-world position to Godot screen coordinates.
pub fn to_godot(position: neogen_core::Vec2) -> Vector2 {
    Vector2::new(position.x as f32, -(position.y as f32))
}

/// Convert Godot screen coordinates back to core-world units (inverse
/// flip; f32 → f64 widening is lossless).
///
/// Currently exercised by the round-trip tests; it is the documented
/// inverse for incoming screen-space input (the phase-5 editor panel
/// will send positions in screen coordinates).
#[allow(dead_code)]
pub fn from_godot(position: Vector2) -> neogen_core::Vec2 {
    neogen_core::Vec2::new(f64::from(position.x), -f64::from(position.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_identity() {
        for (x, y) in [(0.0, 0.0), (3.0, 4.0), (-6.5, 12.25), (1e6, -1e6)] {
            let core = neogen_core::Vec2::new(x, y);
            assert_eq!(from_godot(to_godot(core)), core);
        }
    }

    #[test]
    fn y_is_flipped_x_passes() {
        let screen = to_godot(neogen_core::Vec2::new(3.0, 4.0));
        assert_eq!(screen, Vector2::new(3.0, -4.0));
        assert_eq!(
            from_godot(Vector2::new(3.0, -4.0)),
            neogen_core::Vec2::new(3.0, 4.0)
        );
    }
}
