//! Backlog 1.3 — golden determinism tests.
//!
//! Two guarantees:
//!
//! 1. **Instance determinism**: two independently constructed worlds with
//!    the same seed produce the same state hash at every checkpoint tick.
//! 2. **Golden fixtures**: hashes for fixed seeds at checkpoint ticks are
//!    recorded in `tests/golden/world_hashes.txt` and compared on every
//!    run — catching *any* future change to generation, stepping, or the
//!    state shape, intentional or accidental.
//!
//! # Updating the fixtures
//!
//! Run with the env flag to regenerate `tests/golden/world_hashes.txt`:
//!
//! ```text
//! NEOGEN_UPDATE_GOLDEN=1 cargo test -p neogen-core --test golden_determinism
//! ```
//!
//! This is legitimate ONLY when the state format or simulation semantics
//! changed on purpose (and the change is explained in the commit message).
//! If the fixtures fail without such a change, you introduced
//! non-determinism or a regression — fix the code, do not refresh the
//! fixtures.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use neogen_core::{Command, Vec2, World, state_hash};

/// Seeds recorded in the fixtures.
const SEEDS: [u64; 3] = [42, 7, u64::MAX];
/// Checkpoint ticks where hashes are compared/recorded.
const CHECKPOINTS: [u64; 4] = [1, 10, 100, 1000];

/// Fixed script: (tick when the command is pushed, command factory).
/// Targets derive from the seeded rover start, so they are seed-dependent
/// but fully deterministic (FIX-раунд 1.5 #1: without commands in the
/// golden run, a regression in MoveTo/Scan logic would change no fixture).
fn script_for(start: Vec2) -> Vec<(u64, Command)> {
    vec![
        (
            1,
            Command::MoveTo {
                target: start + Vec2::new(5.0, 5.0),
            },
        ),
        (10, Command::Scan { radius: 4.0 }),
        (50, Command::MoveTo { target: Vec2::ZERO }),
        (500, Command::Scan { radius: 1.5 }),
    ]
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/world_hashes.txt")
}

/// One recorded (seed, tick, hash) triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    seed: u64,
    tick: u64,
    hash: u64,
}

/// Compute all fixture entries: every seed run to the last checkpoint,
/// driven by the fixed script (commands pushed at their scheduled ticks).
fn compute_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for &seed in SEEDS.iter() {
        let mut world = World::new(seed);
        let id = world.rovers().next().expect("rover exists").id();
        let start = world.rovers().next().expect("rover exists").position();
        let script = script_for(start);
        for &checkpoint in CHECKPOINTS.iter() {
            while world.tick() < checkpoint {
                let next_tick = world.tick() + 1;
                let due: Vec<Command> = script
                    .iter()
                    .filter(|(at, _)| *at == next_tick)
                    .map(|(_, command)| *command)
                    .collect();
                if !due.is_empty() {
                    world
                        .push_commands(id, due)
                        .expect("script commands are valid");
                }
                world.step();
            }
            entries.push(Entry {
                seed,
                tick: checkpoint,
                hash: state_hash(world.state()),
            });
        }
    }
    entries
}

/// Render entries as the fixture text: one `seed tick 0xhash` per line.
fn render_fixture(entries: &[Entry]) -> String {
    let mut text = String::new();
    text.push_str("# Neogen golden hashes: seed tick hash\n");
    text.push_str("# Hash = FNV-1a over the WorldState field walk (src/hash.rs).\n");
    text.push_str("# Regenerate ONLY deliberately: NEOGEN_UPDATE_GOLDEN=1 cargo test -p neogen-core --test golden_determinism\n");
    for entry in entries {
        writeln!(
            &mut text,
            "{} {} 0x{:016x}",
            entry.seed, entry.tick, entry.hash
        )
        .expect("formatting into String cannot fail");
    }
    text
}

/// Parse the fixture file back into entries (ignores `#` comments).
fn parse_fixture(text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (seed, tick, hash) = match (parts.next(), parts.next(), parts.next()) {
            (Some(seed), Some(tick), Some(hash)) if parts.next().is_none() => (seed, tick, hash),
            _ => panic!("malformed fixture line: {line:?}"),
        };
        entries.push(Entry {
            seed: seed.parse().expect("seed is u64"),
            tick: tick.parse().expect("tick is u64"),
            hash: u64::from_str_radix(hash.trim_start_matches("0x"), 16).expect("hash is hex u64"),
        });
    }
    entries
}

#[test]
fn golden_hashes_match_fixtures() {
    let actual = compute_entries();
    let path = fixture_path();

    if std::env::var_os("NEOGEN_UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        }
        std::fs::write(&path, render_fixture(&actual))
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        eprintln!("golden fixtures updated: {} entries", actual.len());
        return;
    }

    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}\nif this is a deliberate format change, regenerate with NEOGEN_UPDATE_GOLDEN=1", path.display()));
    let expected = parse_fixture(&text);

    assert_eq!(
        actual, expected,
        "state hashes drifted from the golden fixtures; \
         if this is a deliberate state-format change, regenerate with \
         NEOGEN_UPDATE_GOLDEN=1 and explain the change in the commit"
    );
}

#[test]
fn two_instances_same_seed_same_hash_at_checkpoints() {
    for &seed in SEEDS.iter() {
        // Different construction paths: one direct, one through a state
        // clone, to catch hidden state outside WorldState.
        let mut a = World::new(seed);
        let mut b = World::from_state(World::new(seed).into_state());

        for &checkpoint in CHECKPOINTS.iter() {
            while a.tick() < checkpoint {
                a.step();
                b.step();
            }
            assert_eq!(
                state_hash(a.state()),
                state_hash(b.state()),
                "seed {seed} diverged at tick {checkpoint}"
            );
        }
    }
}

#[test]
fn fixtures_cover_all_seeds_and_checkpoints() {
    if std::env::var_os("NEOGEN_UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        // Update mode races with the writer; the file is validated on the
        // next regular run.
        return;
    }
    // Guard the fixture file itself against accidental truncation.
    let text = std::fs::read_to_string(fixture_path()).expect("fixture file exists");
    let entries = parse_fixture(&text);
    let indexed: BTreeMap<(u64, u64), u64> =
        entries.iter().map(|e| ((e.seed, e.tick), e.hash)).collect();
    assert_eq!(indexed.len(), entries.len(), "duplicate fixture entries");
    for &seed in SEEDS.iter() {
        for &checkpoint in CHECKPOINTS.iter() {
            assert!(
                indexed.contains_key(&(seed, checkpoint)),
                "fixture missing seed {seed} tick {checkpoint}"
            );
        }
    }
}
