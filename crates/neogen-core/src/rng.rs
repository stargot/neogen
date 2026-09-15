//! Deterministic RNG: SplitMix64 seeding + xoshiro256\*\* core.
//!
//! # Why these algorithms
//!
//! - **xoshiro256\*\*\* as the main generator**: fast (a handful of ALU
//!   ops), statistically strong (passes the usual PractRand/big-crush
//!   batteries), with a compact 256-bit state that we can embed in the world
//!   state and later in snapshots. Not cryptographic — irrelevant here, the
//!   RNG drives world generation, not secrets.
//! - **SplitMix64 as the seeder**: expands one `u64` seed into the four
//!   xoshiro state words with excellent avalanche, so nearby seeds produce
//!   unrelated streams. xoshiro must not be seeded with an all-zero state —
//!   SplitMix64 seeding guarantees it never is.
//!
//! # Determinism and format stability
//!
//! Only integer arithmetic (`wrapping_*`, `rotate_left`) is used, so the
//! output word sequence is identical on every platform and every run for a
//! given seed — the golden tests in backlog 1.3 rely on it.
//!
//! **The RNG state is part of the future snapshot format (backlog 1.4):**
//! after worlds can be saved, changing the algorithm, the seeding, or the
//! draw order in generation breaks saved worlds and requires a format
//! version bump + migration. Freeze this module once snapshots land.

use core::ops::Range;

/// SplitMix64 — used only to expand a seed into xoshiro state words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// xoshiro256\*\* generator seeded from a single `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Create a generator from a seed. Same seed → same output sequence,
    /// on any platform; different seeds → independent sequences.
    pub fn from_seed(seed: u64) -> Self {
        let mut seeder = SplitMix64 { state: seed };
        let mut s = [0u64; 4];
        for word in &mut s {
            *word = seeder.next();
        }
        Self { s }
    }

    /// Next raw 64-bit value (xoshiro256\*\* step).
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Next `f64` in `[0, 1)`: top 53 bits of a raw draw, scaled by 2⁻⁵³.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
    }

    /// Next `u64` in `range` (`[start, end)`), unbiased via rejection:
    /// draws outside the largest exact multiple of the range width are
    /// discarded and redrawn.
    ///
    /// # Panics
    /// If the range is empty (`start >= end`).
    pub fn next_u64_in(&mut self, range: Range<u64>) -> u64 {
        assert!(range.start < range.end, "empty range: {range:?}");
        let width = (range.end - range.start) as u128;
        // Draw rejection threshold: values below it map onto the width
        // exactly uniformly (2^64 % width leftover values are rejected).
        let threshold = (1u128 << 64) - ((1u128 << 64) % width);
        loop {
            let x = self.next_u64() as u128;
            if x < threshold {
                return range.start + (x % width) as u64;
            }
        }
    }

    /// Next `f64` in `range` (`[start, end)`), linearly mapped from
    /// [`next_f64`](Self::next_f64). Floating rounding may in principle
    /// return the `end` boundary on the very last representable step; treat
    /// the interval as "essentially half-open".
    ///
    /// # Panics
    /// If the range is empty (`start >= end`).
    pub fn next_f64_in(&mut self, range: Range<f64>) -> f64 {
        assert!(range.start < range.end, "empty range: {range:?}");
        range.start + self.next_f64() * (range.end - range.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden values for `from_seed(42)` — cross-checked against reference
    /// SplitMix64 + xoshiro256\*\* implementations. If this test fails, the
    /// algorithm changed and the snapshot format broke with it (see module
    /// docs).
    #[test]
    fn golden_sequence_for_seed_42() {
        let mut rng = Rng::from_seed(42);
        assert_eq!(rng.next_u64(), 0x1578_0B2E_0C2E_C716);
        assert_eq!(rng.next_u64(), 0x6104_D986_6D11_3A7E);
        assert_eq!(rng.next_u64(), 0xAE17_5332_39E4_99A1);
        assert_eq!(rng.next_u64(), 0xECB8_AD47_03B3_60A1);
    }

    #[test]
    fn golden_first_value_for_seed_7() {
        let mut rng = Rng::from_seed(7);
        assert_eq!(rng.next_u64(), 0xB358_FAF7_4EF9_765A);
    }

    #[test]
    fn recreated_rng_repeats_the_sequence() {
        let mut a = Rng::from_seed(987);
        let mut b = Rng::from_seed(987);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
        assert_ne!(a, b);
    }

    #[test]
    fn f64_draws_stay_in_unit_interval() {
        let mut rng = Rng::from_seed(5);
        let mut above_half = 0;
        let mut max = 0.0_f64;
        for _ in 0..10_000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "out of [0, 1): {v}");
            if v > 0.5 {
                above_half += 1;
            }
            max = max.max(v);
        }
        // Spread check: a mapping bug (e.g. from_bits misuse) collapses all
        // draws to ~0 while still "in range".
        assert!(
            (4000..6000).contains(&above_half),
            "skewed: {above_half}/10000 above 0.5"
        );
        assert!(max > 0.99, "never reached the top of the interval: {max}");
    }

    #[test]
    fn u64_draws_respect_range_and_spread() {
        let mut rng = Rng::from_seed(9);
        let mut seen = [false; 10];
        for _ in 0..10_000 {
            let v = rng.next_u64_in(10..20);
            assert!((10..20).contains(&v));
            seen[(v - 10) as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "narrow range never covered");
    }

    #[test]
    fn f64_draws_respect_range() {
        let mut rng = Rng::from_seed(11);
        for _ in 0..10_000 {
            let v = rng.next_f64_in(-8.0..8.0);
            assert!((-8.0..8.0).contains(&v), "out of [-8, 8): {v}");
        }
    }

    #[test]
    #[should_panic(expected = "empty range")]
    fn empty_ranges_panic() {
        let mut rng = Rng::from_seed(1);
        let _ = rng.next_u64_in(5..5);
    }
}
