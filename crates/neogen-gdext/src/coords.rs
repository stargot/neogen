//! Coordinate conversion between the simulation core and Godot.
//!
//! The core world is top-down with **Y pointing north** (math convention,
//! f64 world units). Godot 2D has **Y pointing down** (screen convention)
//! and uses f32. `to_godot` is the single place where this rule lives:
//!
//! - X passes through unchanged;
//! - **Y is flipped** (`-y`) so that "north" in the world reads as "up" on
//!   screen;
//! - f64 → f32 narrowing happens here (rendering precision is enough; the
//!   authoritative state stays in the core).

use godot::builtin::Vector2;

/// Convert a core-world position to Godot screen coordinates.
pub fn to_godot(position: neogen_core::Vec2) -> Vector2 {
    Vector2::new(position.x as f32, -(position.y as f32))
}
