//! Backlog 1.4 — snapshot roundtrip and resume equivalence.

use neogen_core::{SNAPSHOT_VERSION, SnapshotError, Vec2, World, from_bytes, state_hash, to_bytes};

#[test]
fn roundtrip_preserves_state_and_hash() {
    for seed in [42u64, 7, u64::MAX] {
        let mut world = World::new(seed);
        for _ in 0..250 {
            world.step();
        }
        let bytes = to_bytes(world.state());
        let restored = from_bytes(&bytes).expect("valid snapshot");
        assert_eq!(restored, *world.state(), "seed {seed}: state drifted");
        assert_eq!(state_hash(&restored), state_hash(world.state()));
    }
}

#[test]
fn roundtrip_with_multiple_rovers() {
    let mut state = World::new(5).into_state();
    state.spawn_rover(Vec2::new(1.0, -2.0), 1.25);
    state.spawn_rover(Vec2::new(-3.5, 4.0), 2.5);

    let bytes = to_bytes(&state);
    let restored = from_bytes(&bytes).expect("valid snapshot");
    assert_eq!(restored, state);
    assert_eq!(restored.rovers().count(), 3);

    // The issuer watermark survives: new ids continue the sequence.
    let mut restored = restored;
    let next = restored.spawn_rover(Vec2::ZERO, 0.0);
    assert_eq!(next.raw(), 4);
}

#[test]
fn snapshot_resume_matches_continuous_run() {
    const K: u64 = 100;
    const N: u64 = 100;
    for seed in [42u64, 7] {
        // Uninterrupted reference run.
        let mut continuous = World::new(seed);
        for _ in 0..(K + N) {
            continuous.step();
        }

        // Save at K, resume from bytes, continue N more ticks.
        let mut saved = World::new(seed);
        for _ in 0..K {
            saved.step();
        }
        let bytes = to_bytes(saved.state());
        let mut resumed = World::from_state(from_bytes(&bytes).expect("valid snapshot"));
        assert_eq!(resumed.tick(), K);
        for _ in 0..N {
            saved.step();
            resumed.step();
        }

        assert_eq!(resumed.tick(), K + N);
        assert_eq!(
            state_hash(resumed.state()),
            state_hash(continuous.state()),
            "seed {seed}: resumed run diverged from the continuous one"
        );
        assert_eq!(resumed.state(), continuous.state());
        // And the saved original (kept in memory, no snapshot involved)
        // agrees with both.
        assert_eq!(state_hash(saved.state()), state_hash(continuous.state()));
    }
}

#[test]
fn header_layout_is_magic_then_version_lsb() {
    let bytes = to_bytes(World::new(1).state());
    assert_eq!(&bytes[0..4], b"NEGN");
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("4 bytes"));
    assert_eq!(version, SNAPSHOT_VERSION);
    assert_eq!(SNAPSHOT_VERSION, 1);
}

#[test]
fn bad_magic_is_rejected() {
    let mut bytes = to_bytes(World::new(1).state());
    bytes[0] = b'X';
    match from_bytes(&bytes) {
        Err(SnapshotError::BadMagic { found }) => assert_eq!(found, *b"XEGN"),
        other => panic!("expected BadMagic, got {other:?}"),
    }
}

#[test]
fn unsupported_version_is_rejected() {
    let mut bytes = to_bytes(World::new(1).state());
    bytes[4..8].copy_from_slice(&999u32.to_le_bytes());
    match from_bytes(&bytes) {
        Err(SnapshotError::UnsupportedVersion { found }) => assert_eq!(found, 999),
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn truncated_buffers_are_rejected() {
    let bytes = to_bytes(World::new(1).state());
    for len in [0, 1, 3, 4, 7, 8, 15, 63, bytes.len() - 1] {
        match from_bytes(&bytes[..len]) {
            Err(SnapshotError::UnexpectedEof { needed, available }) => {
                assert!(
                    needed > available,
                    "needed {needed} <= available {available}"
                );
                assert_eq!(available, len);
            }
            other => panic!("len {len}: expected UnexpectedEof, got {other:?}"),
        }
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = to_bytes(World::new(1).state());
    bytes.extend_from_slice(&[0u8, 0, 0]);
    match from_bytes(&bytes) {
        Err(SnapshotError::TrailingBytes { extra }) => assert_eq!(extra, 3),
        other => panic!("expected TrailingBytes, got {other:?}"),
    }
}

#[test]
fn semantically_impossible_content_is_rejected() {
    // Rover id 0 (first rover id sits right after the header + counts).
    let mut bytes = to_bytes(World::new(1).state());
    let id_offset = 8 + 8 + 8 + 32 + 8; // magic+version, tick, seed, rng, count
    bytes[id_offset..id_offset + 4].copy_from_slice(&0u32.to_le_bytes());
    match from_bytes(&bytes) {
        Err(SnapshotError::Malformed { reason }) => assert!(reason.contains("reserved")),
        other => panic!("expected Malformed, got {other:?}"),
    }

    // Issuer watermark patched to 0 (last four bytes).
    let mut bytes = to_bytes(World::new(1).state());
    let n = bytes.len();
    bytes[n - 4..].copy_from_slice(&0u32.to_le_bytes());
    match from_bytes(&bytes) {
        Err(SnapshotError::Malformed { reason }) => assert!(reason.contains("reserved")),
        other => panic!("expected Malformed, got {other:?}"),
    }

    // Watermark below an existing rover id.
    let mut bytes = to_bytes(World::new(1).state());
    let n = bytes.len();
    bytes[n - 4..].copy_from_slice(&1u32.to_le_bytes());
    match from_bytes(&bytes) {
        Err(SnapshotError::Malformed { reason }) => assert!(reason.contains("watermark")),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn error_messages_are_readable() {
    let bad = SnapshotError::BadMagic { found: *b"JUNK" }.to_string();
    assert!(bad.contains("JUNK"), "{bad}");
    let old = SnapshotError::UnsupportedVersion { found: 999 }.to_string();
    assert!(old.contains("999"), "{old}");
    let eof = SnapshotError::UnexpectedEof {
        needed: 10,
        available: 4,
    }
    .to_string();
    assert!(eof.contains("10") && eof.contains("4"), "{eof}");
}
