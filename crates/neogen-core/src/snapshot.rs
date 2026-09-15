//! Snapshot serialization of [`WorldState`] (backlog 1.4).
//!
//! # Byte format (schema version 2)
//!
//! Hand-rolled, little-endian (LSB-first) everywhere, no external codecs:
//!
//! ```text
//! offset  size  field
//! 0       4     magic: ASCII "NEGN"
//! 4       4     schema version: u32, currently 2
//! 8       8     tick: u64
//! 16      8     seed: u64
//! 24      32    rng state words s0..s3: 4 × u64
//! 56      8     rover count: u64
//! 64      44·n  rovers, ascending id, each:
//!               +0   id: u32
//!               +4   position.x: f64 bits
//!               +12  position.y: f64 bits
//!               +20  heading.x: f64 bits (direction vector)
//!               +28  heading.y: f64 bits
//!               +36  speed: f64 bits
//! …       4     next rover id (id-issuer state): u32
//! ```
//!
//! The body mirrors the [`crate::hash`] state walk order (tick, seed, RNG
//! words, rover count, rovers field-by-field) so the two formats cannot
//! silently diverge; the trailing `next rover id` is snapshot-only (the hash
//! intentionally excludes the id issuer) but is required for a faithful
//! restore — without it, resumed worlds would re-issue already-used ids.
//!
//! Version history: **1** — initial layout, heading as one f64 angle in
//! radians; **2** — heading as a direction vector (two f64s) for
//! cross-platform bit-determinism (FIX-раунд 1.5 #2).
//!
//! **v2 stub decision:** command queues, scan buffers and in-progress
//! command progress are *not* serialized. Queues are inputs (a resumed
//! world starts with empty queues — scripts re-attach in phase 5.4),
//! buffers are derived data. The trailing issuer watermark *is* kept so
//! restored worlds never re-issue ids. Revisit deliberately when saves
//! become player-facing (backlog 9.x).

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt;

use crate::ids::RoverId;
use crate::math::Vec2;
use crate::rng::Rng;
use crate::world::{Rover, WorldState};

/// Magic bytes at the start of every snapshot: ASCII `NEGN`.
pub const MAGIC: [u8; 4] = *b"NEGN";

/// Current snapshot schema version. [`from_bytes`] accepts only this one.
///
/// v2: heading as direction vector (two f64s) — see module docs.
pub const SNAPSHOT_VERSION: u32 = 2;

/// Size of one serialized rover: id (u32) + five f64 bit patterns.
const ROVER_BYTES: u64 = 4 + 5 * 8;

/// Failures of [`from_bytes`] — input is never trusted, each variant says
/// exactly what was wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// First four bytes are not `NEGN`.
    BadMagic {
        /// The four bytes actually found.
        found: [u8; 4],
    },
    /// Schema version is not [`SNAPSHOT_VERSION`].
    UnsupportedVersion {
        /// Version found in the header.
        found: u32,
    },
    /// Buffer ends before the announced structure does.
    UnexpectedEof {
        /// Total length a complete snapshot of this shape would need.
        needed: usize,
        /// Actual buffer length.
        available: usize,
    },
    /// Well-formed snapshot followed by extra bytes.
    TrailingBytes {
        /// Number of unread bytes after the snapshot.
        extra: usize,
    },
    /// Structurally valid bytes, semantically impossible content.
    Malformed {
        /// What exactly is wrong.
        reason: &'static str,
    },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic { found } => {
                let shown: String = found.iter().map(|&b| b as char).collect();
                write!(f, "bad magic {shown:?} (expected \"NEGN\")")
            }
            Self::UnsupportedVersion { found } => write!(
                f,
                "unsupported snapshot schema version {found} (supported: {SNAPSHOT_VERSION})"
            ),
            Self::UnexpectedEof { needed, available } => {
                write!(
                    f,
                    "snapshot ends early: needed {needed} bytes, available {available}"
                )
            }
            Self::TrailingBytes { extra } => {
                write!(f, "{extra} trailing bytes after the snapshot")
            }
            Self::Malformed { reason } => write!(f, "malformed snapshot: {reason}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_f64(out: &mut Vec<u8>, value: f64) {
    write_u64(out, value.to_bits());
}

/// Serialize the full world state (see the byte format in the module docs).
pub fn to_bytes(state: &WorldState) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + 36 + 4);
    out.extend_from_slice(&MAGIC);
    write_u32(&mut out, SNAPSHOT_VERSION);
    write_u64(&mut out, state.tick);
    write_u64(&mut out, state.seed);
    for word in state.rng().state_words() {
        write_u64(&mut out, word);
    }
    write_u64(&mut out, state.rovers().count() as u64);
    for rover in state.rovers() {
        write_u32(&mut out, rover.id.raw());
        write_f64(&mut out, rover.position.x);
        write_f64(&mut out, rover.position.y);
        write_f64(&mut out, rover.heading.x);
        write_f64(&mut out, rover.heading.y);
        write_f64(&mut out, rover.speed);
    }
    write_u32(&mut out, state.next_rover_id());
    out
}

/// Cursor over the input buffer for [`from_bytes`].
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn new(data: &[u8]) -> Reader<'_> {
        Reader { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&[u8], SnapshotError> {
        if self.remaining() < n {
            return Err(SnapshotError::UnexpectedEof {
                needed: self.pos + n,
                available: self.data.len(),
            });
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, SnapshotError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("take(4) yields 4 bytes");
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64(&mut self) -> Result<u64, SnapshotError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("take(8) yields 8 bytes");
        Ok(u64::from_le_bytes(bytes))
    }
}

/// Deserialize a world state produced by [`to_bytes`].
///
/// Strictly validates everything the header and structure promise: magic,
/// schema version, buffer length, per-rover plausibility (ids non-zero,
/// unique, below the issuer watermark), and no trailing bytes.
pub fn from_bytes(data: &[u8]) -> Result<WorldState, SnapshotError> {
    let mut r = Reader::new(data);

    let magic: [u8; 4] = r.take(4)?.try_into().expect("take(4) yields 4 bytes");
    if magic != MAGIC {
        return Err(SnapshotError::BadMagic { found: magic });
    }
    let version = r.read_u32()?;
    if version != SNAPSHOT_VERSION {
        return Err(SnapshotError::UnsupportedVersion { found: version });
    }

    let tick = r.read_u64()?;
    let seed = r.read_u64()?;
    let mut words = [0u64; 4];
    for word in &mut words {
        *word = r.read_u64()?;
    }
    let rng = Rng::from_state_words(words).ok_or(SnapshotError::Malformed {
        reason: "rng state words are all zero",
    })?;

    let count = r.read_u64()?;
    // Sanity-check the announced count against the remaining bytes before
    // allocating anything (a garbage u64 must not turn into a huge map).
    let needed = count
        .checked_mul(ROVER_BYTES)
        .ok_or(SnapshotError::Malformed {
            reason: "rover count overflows u64",
        })?;
    let needed = needed
        .checked_add(4) // trailing issuer watermark
        .ok_or(SnapshotError::Malformed {
            reason: "rover count overflows u64",
        })?;
    if needed > r.remaining() as u64 {
        return Err(SnapshotError::UnexpectedEof {
            needed: r.pos + needed as usize,
            available: data.len(),
        });
    }

    let mut rovers = BTreeMap::new();
    for _ in 0..count {
        let raw_id = r.read_u32()?;
        if raw_id == 0 {
            return Err(SnapshotError::Malformed {
                reason: "rover id 0 is reserved",
            });
        }
        let id = RoverId::from_raw(raw_id);
        let x = f64::from_bits(r.read_u64()?);
        let y = f64::from_bits(r.read_u64()?);
        let heading_x = f64::from_bits(r.read_u64()?);
        let heading_y = f64::from_bits(r.read_u64()?);
        let speed = f64::from_bits(r.read_u64()?);
        let rover = Rover {
            id,
            position: Vec2::new(x, y),
            heading: Vec2::new(heading_x, heading_y),
            speed,
            commands: VecDeque::new(),
            scan_buffer: Vec::new(),
            remaining_ticks: 0,
        };
        if rovers.insert(id, rover).is_some() {
            return Err(SnapshotError::Malformed {
                reason: "duplicate rover id",
            });
        }
    }

    let next_rover_id = r.read_u32()?;
    if next_rover_id == 0 {
        return Err(SnapshotError::Malformed {
            reason: "next rover id 0 is reserved",
        });
    }
    if let Some(max_id) = rovers.keys().next_back()
        && max_id.raw() >= next_rover_id
    {
        return Err(SnapshotError::Malformed {
            reason: "rover id beyond the id-issuer watermark",
        });
    }

    if r.remaining() != 0 {
        return Err(SnapshotError::TrailingBytes {
            extra: r.remaining(),
        });
    }

    Ok(WorldState::from_parts(
        tick,
        seed,
        rovers,
        next_rover_id,
        rng,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn roundtrip_via_unit_smoke() {
        let world = World::new(42);
        let bytes = to_bytes(world.state());
        assert_eq!(&bytes[0..4], b"NEGN");
        let restored = from_bytes(&bytes).expect("valid snapshot");
        assert_eq!(restored, *world.state());
    }
}
