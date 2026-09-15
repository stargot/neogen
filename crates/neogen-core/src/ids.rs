//! Typed entity ids and their deterministic issuer.

use core::fmt;

/// Id of a rover entity.
///
/// Newtype around a raw `u32`: cheap to `Copy`, totally ordered, and a
/// distinct type so rover ids can never be silently mixed with future id
/// spaces (stations, scripts, …) even though they share the sequential-issue
/// pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoverId(u32);

impl RoverId {
    /// Wrap a raw id (mainly for snapshot decoding).
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Raw numeric value of the id.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for RoverId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rover#{}", self.0)
    }
}

/// Sequential id counter for one world.
///
/// Deterministic issuance by construction: ids are handed out strictly in
/// ascending order and never reused, so two worlds performing the same spawn
/// sequence get identical id sequences — independent of the seed. Id `0` is
/// never issued (reserved as an "empty" value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdIssuer {
    next: u32,
}

impl IdIssuer {
    /// Issuer whose first issued id is `1`.
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// Issue the next id.
    pub fn issue(&mut self) -> RoverId {
        let id = RoverId(self.next);
        self.next = self
            .next
            .checked_add(1)
            .expect("entity id space exhausted (u32)");
        id
    }
}

impl Default for IdIssuer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sequential_from_one() {
        let mut issuer = IdIssuer::new();
        assert_eq!(issuer.issue(), RoverId::from_raw(1));
        assert_eq!(issuer.issue(), RoverId::from_raw(2));
        assert_eq!(issuer.issue(), RoverId::from_raw(3));
    }

    #[test]
    fn cloned_issuers_continue_the_same_sequence() {
        let mut a = IdIssuer::new();
        a.issue();
        a.issue();
        let mut b = a;
        assert_eq!(a.issue(), b.issue());
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(RoverId::from_raw(7).to_string(), "rover#7");
    }
}
