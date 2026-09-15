//! Log buffer for script output (backlog 2.4).
//!
//! Player-facing `print` writes here instead of stdout; the bridge drains
//! the buffer into the game console (phase 3.4). Bounded ring semantics,
//! same eviction policy as `MAX_SCAN_BUFFER` in the core: when full, the
//! oldest entries are dropped — a chatty script must not grow memory
//! without bound, and dropping history is preferred over blocking.

use std::collections::VecDeque;

use neogen_core::RoverId;

/// Maximum entries kept per buffer (oldest evicted on overflow).
pub const MAX_LOG_BUFFER: usize = 256;

/// One console line produced by a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// World tick at which the line was produced.
    pub tick: u64,
    /// Id of the script that produced the line.
    pub script_id: u32,
    /// Rover the script is attached to.
    pub rover_id: RoverId,
    /// The rendered text (arguments joined like Lua's `print`).
    pub text: String,
}

/// Bounded FIFO of [`LogEntry`] (ring: oldest evicted).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
}

impl LogBuffer {
    /// New empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry, evicting the oldest when over capacity.
    pub fn push(&mut self, entry: LogEntry) {
        self.entries.push_back(entry);
        while self.entries.len() > MAX_LOG_BUFFER {
            self.entries.pop_front();
        }
    }

    /// Take all entries out (read → cleared), oldest first.
    pub fn drain(&mut self) -> Vec<LogEntry> {
        self.entries.drain(..).collect()
    }

    /// Copy of the current entries, oldest first (for the bridge/tests).
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Number of entries currently held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the buffer holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tick: u64) -> LogEntry {
        LogEntry {
            tick,
            script_id: 1,
            rover_id: RoverId::from_raw(1),
            text: format!("line {tick}"),
        }
    }

    #[test]
    fn push_drain_snapshot_roundtrip() {
        let mut buffer = LogBuffer::new();
        buffer.push(entry(1));
        buffer.push(entry(2));
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.snapshot().len(), 2);

        let drained = buffer.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].tick, 1);
        assert!(buffer.is_empty());
        assert!(buffer.drain().is_empty());
    }

    #[test]
    fn overflow_evicts_oldest() {
        let mut buffer = LogBuffer::new();
        for tick in 0..=(MAX_LOG_BUFFER as u64) {
            buffer.push(entry(tick));
        }
        assert_eq!(buffer.len(), MAX_LOG_BUFFER);
        // Entry 0 was evicted; the buffer starts at 1.
        assert_eq!(buffer.snapshot()[0].tick, 1);
        assert_eq!(
            buffer.snapshot().last().unwrap().tick,
            MAX_LOG_BUFFER as u64
        );
    }
}
