//! Backlog 2.6 — deterministic multi-script execution.
//!
//! Three scripts drive three rovers in one world:
//! - script 1 (rover 1): two sequential moves (long and medium distance);
//! - script 2 (rover 2): eternal scan+print loop that yields every tick;
//! - script 3 (rover 3): one short move, then finished.
//!
//! The position layout at checkpoint ticks (10/50/100) is recorded in
//! `tests/golden/script_layout.txt` as **bit-exact f64 patterns** and
//! compared on every run; the world state hash at tick 100 must also be
//! identical across independent runs and survive a same-source restart of
//! the scanner script.
//!
//! Regenerate the fixture ONLY deliberately (format/semantics change):
//! ```text
//! NEOGEN_UPDATE_GOLDEN=1 cargo test -p neogen-script --test multi_script
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use neogen_core::{RoverId, Vec2, World};
use neogen_script::{RuntimeConfig, ScriptHost, ScriptState};

const CHECKPOINTS: [u64; 3] = [10, 50, 100];
const SCRIPT_SCANNER: &str = "while true do scan(2.0) print(\"patrol\") coroutine.yield() end";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/script_layout.txt")
}

/// World with three rovers: 1 (seeded pad position), 2 at (-5,-5), 3 at (5,5).
fn build_world() -> World {
    let mut state = World::new(42).into_state();
    state.spawn_rover(Vec2::new(-5.0, -5.0), Vec2::new(1.0, 0.0));
    state.spawn_rover(Vec2::new(5.0, 5.0), Vec2::new(0.0, 1.0));
    World::from_state(state)
}

/// Attach the three scripts in a fixed order (ids 1..=3).
fn attach_scripts(host: &mut ScriptHost) {
    let [mover, scanner, finisher] = host
        .rover_ids()
        .try_into()
        .expect("three rovers in ascending order");
    host.attach_script(mover, "move(12, 0) move(12, 8)")
        .expect("mover attaches");
    host.attach_script(scanner, SCRIPT_SCANNER)
        .expect("scanner attaches");
    host.attach_script(finisher, "move(3, -4) print(\"done\")")
        .expect("finisher attaches");
}

/// Run to every checkpoint, collecting (tick → rover id → position bits).
fn compute_layout() -> BTreeMap<u64, Vec<(RoverId, u64, u64)>> {
    let mut host = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host creates");
    attach_scripts(&mut host);

    let mut layout = BTreeMap::new();
    for &checkpoint in CHECKPOINTS.iter() {
        while host.world_tick() < checkpoint {
            host.step_world();
        }
        let snapshot: Vec<_> = host
            .rover_ids()
            .into_iter()
            .map(|id| {
                let position = host.rover_position(id).expect("rover exists");
                (id, position.x.to_bits(), position.y.to_bits())
            })
            .collect();
        layout.insert(checkpoint, snapshot);
    }
    layout
}

fn render_fixture(layout: &BTreeMap<u64, Vec<(RoverId, u64, u64)>>) -> String {
    let mut text = String::new();
    text.push_str("# Neogen multi-script golden layout: tick rover_id x_bits y_bits\n");
    text.push_str("# Positions as bit-exact f64 patterns; regenerate deliberately with\n");
    text.push_str("# NEOGEN_UPDATE_GOLDEN=1 cargo test -p neogen-script --test multi_script\n");
    for (tick, rovers) in layout {
        for (id, x_bits, y_bits) in rovers {
            writeln!(
                &mut text,
                "{tick} {} {x_bits:#018x} {y_bits:#018x}",
                id.raw()
            )
            .unwrap();
        }
    }
    text
}

fn parse_fixture(text: &str) -> BTreeMap<u64, Vec<(RoverId, u64, u64)>> {
    let mut layout = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (tick, id, x, y) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(t), Some(i), Some(x), Some(y)) if parts.next().is_none() => (t, i, x, y),
            _ => panic!("malformed fixture line: {line:?}"),
        };
        layout
            .entry(tick.parse().expect("tick is u64"))
            .or_insert_with(Vec::new)
            .push((
                RoverId::from_raw(id.parse().expect("rover id is u32")),
                u64::from_str_radix(x.trim_start_matches("0x"), 16).expect("x bits"),
                u64::from_str_radix(y.trim_start_matches("0x"), 16).expect("y bits"),
            ));
    }
    layout
}

#[test]
fn layout_matches_golden_fixture() {
    let actual = compute_layout();
    let path = fixture_path();

    if std::env::var_os("NEOGEN_UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("fixture dir");
        }
        std::fs::write(&path, render_fixture(&actual)).expect("write fixture");
        eprintln!("multi-script golden fixture updated");
        return;
    }

    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nregenerate with NEOGEN_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let expected = parse_fixture(&text);
    assert_eq!(
        actual, expected,
        "script layout drifted from the golden fixture; \
         if the scheduler semantics changed deliberately, regenerate with \
         NEOGEN_UPDATE_GOLDEN=1 and explain it in the commit"
    );
}

#[test]
fn world_hash_at_tick_100_is_identical_across_runs() {
    let mut run_a = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host");
    let mut run_b = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host");
    attach_scripts(&mut run_a);
    attach_scripts(&mut run_b);

    while run_a.world_tick() < 100 {
        run_a.step_world();
        run_b.step_world();
    }
    assert_eq!(run_a.world_state_hash(), run_b.world_state_hash());
    // And the hash is not the trivial "nothing ever moved" one.
    let mut idle = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host");
    while idle.world_tick() < 100 {
        idle.step_world();
    }
    assert_ne!(run_a.world_state_hash(), idle.world_state_hash());
}

#[test]
fn same_source_restart_does_not_change_the_hash() {
    // Uninterrupted reference.
    let mut reference = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host");
    attach_scripts(&mut reference);
    while reference.world_tick() < 100 {
        reference.step_world();
    }

    // Same world, but the scanner script is restarted (same source) at 50.
    let mut restarted = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host");
    attach_scripts(&mut restarted);
    while restarted.world_tick() < 50 {
        restarted.step_world();
    }
    let scanner_id = 2; // ids 1..=3 in attach order
    assert_eq!(
        restarted.script_state(scanner_id),
        Some(ScriptState::Running)
    );
    restarted
        .restart_script(scanner_id, SCRIPT_SCANNER)
        .expect("restart compiles");
    while restarted.world_tick() < 100 {
        restarted.step_world();
    }

    assert_eq!(
        reference.world_state_hash(),
        restarted.world_state_hash(),
        "restarting the scanner with the same source changed the world"
    );
}

#[test]
fn script_states_diverge_as_expected() {
    let mut host = ScriptHost::new(build_world(), RuntimeConfig::default()).expect("host");
    attach_scripts(&mut host);
    while host.world_tick() < 100 {
        host.step_world();
    }
    assert_eq!(host.script_state(1), Some(ScriptState::Finished));
    assert_eq!(host.script_state(2), Some(ScriptState::Running));
    assert_eq!(host.script_state(3), Some(ScriptState::Finished));

    // The scanner keeps printing; the log is bounded.
    let logs = host.log_snapshot();
    assert!(
        logs.iter().any(|e| e.script_id == 2 && e.text == "patrol"),
        "{logs:?}"
    );
    assert!(
        logs.iter().any(|e| e.script_id == 3 && e.text == "done"),
        "{logs:?}"
    );
    assert!(logs.len() <= neogen_script::MAX_LOG_BUFFER);
}
