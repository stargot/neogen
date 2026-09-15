//! Backlog 1.2 — seed-based deterministic world generation.

use neogen_core::{Rng, RoverId, START_PAD_HALF_SIZE, World, generate_world};

#[test]
fn same_seed_generates_identical_worlds() {
    let a = World::new(1234);
    let b = World::new(1234);
    // Full state equality: rover position, heading, ids, RNG position.
    assert_eq!(a.state(), b.state());

    let rover_a = a.rovers().next().expect("rover exists");
    let rover_b = b.rovers().next().expect("rover exists");
    assert_eq!(rover_a.position, rover_b.position);
    assert_eq!(rover_a.heading, rover_b.heading);
    assert_eq!(rover_a.id, rover_b.id);
    assert_eq!(rover_a.id, RoverId::from_raw(1));
}

#[test]
fn same_seed_worlds_stay_identical_while_ticking() {
    let mut a = World::new(777);
    let mut b = World::new(777);
    for _ in 0..100 {
        a.step();
        b.step();
    }
    assert_eq!(a.state(), b.state());
}

#[test]
fn different_seeds_generate_different_worlds() {
    let a = World::new(1);
    let b = World::new(2);
    assert_ne!(a.state(), b.state());

    let pos_a = a.rovers().next().expect("rover exists").position;
    let pos_b = b.rovers().next().expect("rover exists").position;
    assert_ne!(pos_a, pos_b);
    // ...but id issuance stays seed-independent.
    assert_eq!(
        a.rovers().next().expect("rover exists").id,
        b.rovers().next().expect("rover exists").id
    );
}

#[test]
fn rover_spawns_on_the_starting_pad() {
    for seed in [0, 1, 42, u64::MAX] {
        let world = World::new(seed);
        let rover = world.rovers().next().expect("rover exists");
        assert!((-START_PAD_HALF_SIZE..START_PAD_HALF_SIZE).contains(&rover.position.x));
        assert!((-START_PAD_HALF_SIZE..START_PAD_HALF_SIZE).contains(&rover.position.y));
        assert_eq!(rover.speed, 0.0);
    }
}

#[test]
fn generate_world_matches_world_new() {
    // `World::new` must be exactly `from_state(generate_world(seed))`.
    assert_eq!(World::new(9).into_state(), generate_world(9));
}

#[test]
fn recreated_rng_repeats_the_stream() {
    fn stream(seed: u64, n: usize) -> Vec<u64> {
        let mut rng = Rng::from_seed(seed);
        (0..n).map(|_| rng.next_u64()).collect()
    }
    assert_eq!(stream(2024, 16), stream(2024, 16));
    assert_ne!(stream(2024, 16), stream(2025, 16));
}
