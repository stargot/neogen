//! Minimal 2D vector math.
//!
//! Hand-rolled on purpose: `neogen-core` is a dependency-free island
//! (architecture gate — no glam/nalgebra), so it ships just the handful of
//! operations the simulation needs. Coordinates are `f64` world units.

use core::ops::{Add, Div, Mul, Sub};

/// A 2D vector / point in world space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// Component along the world X axis.
    pub x: f64,
    /// Component along the world Y axis.
    pub y: f64,
}

impl Vec2 {
    /// The zero vector (world origin).
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// A vector with the given components.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Unit vector pointing along `radians` (0 = +X, π/2 = +Y).
    ///
    /// Uses libm `sin`/`cos`, whose last bit may differ between C
    /// libraries — therefore for tests and UI glue only, never for values
    /// that enter simulation state (hash/snapshot). The simulation rotates
    /// via [`normalized`](Self::normalized) and basic IEEE ops instead.
    pub fn from_angle(radians: f64) -> Self {
        Self::new(radians.cos(), radians.sin())
    }

    /// Normalize to a (approximately) unit vector.
    ///
    /// Deterministic across platforms: only `+ − × ÷ √` are used — the
    /// five IEEE 754 basic operations, each correctly rounded, so any
    /// compliant hardware yields identical bits. The zero vector maps to
    /// `+X` by convention.
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length == 0.0 {
            Self::new(1.0, 0.0)
        } else {
            self / length
        }
    }

    /// Dot product with `other`.
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Squared length (cheaper than [`length`](Self::length), no sqrt).
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    /// Euclidean length.
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    /// Euclidean distance to `other`.
    pub fn distance_to(self, other: Self) -> f64 {
        (self - other).length()
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f64> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        let a = Vec2::new(3.0, 4.0);
        assert_eq!(a.length(), 5.0);
        assert_eq!(a + Vec2::new(1.0, 1.0), Vec2::new(4.0, 5.0));
        assert_eq!(a - a, Vec2::ZERO);
        assert_eq!(a * 2.0, Vec2::new(6.0, 8.0));
        assert_eq!(a / 2.0, Vec2::new(1.5, 2.0));
        assert_eq!(a.dot(Vec2::new(1.0, 0.0)), 3.0);
        assert_eq!(a.distance_to(a), 0.0);
    }

    #[test]
    fn from_angle_points_along_axes() {
        let px = Vec2::from_angle(0.0);
        assert!((px.x - 1.0).abs() < 1e-12 && px.y.abs() < 1e-12);
        let py = Vec2::from_angle(core::f64::consts::FRAC_PI_2);
        assert!(py.x.abs() < 1e-12 && (py.y - 1.0).abs() < 1e-12);
    }
}
