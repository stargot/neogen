//! Backlog 1.1 — fixed-timestep tick, cloneable state, deterministic ids.

use core::f64::consts::FRAC_PI_2;

use neogen_core::{IdIssuer, RoverId, TICK_DT, TICK_HZ, Vec2, World};

#[test]
fn thousand_steps_advance_tick_counter_exactly() {
    let mut world = World::new(42);
    assert_eq!(world.tick(), 0);
    for _ in 0..1000 {
        world.step();
    }
    assert_eq!(world.tick(), 1000);
    assert_eq!(world.state().tick, 1000);
}

#[test]
fn tick_dt_matches_frequency() {
    assert_eq!(TICK_DT, 1.0 / f64::from(TICK_HZ));
}

#[test]
fn state_clone_is_identical_snapshot() {
    let mut world = World::new(3);
    for _ in 0..10 {
        world.step();
    }
    let snapshot = world.state().clone();
    // Identical to its source at the moment of cloning.
    assert_eq!(&snapshot, world.state());

    // The clone is frozen; the original keeps ticking.
    for _ in 0..5 {
        world.step();
    }
    assert_eq!(snapshot.tick, 10);
    assert_eq!(world.tick(), 15);
    assert_ne!(&snapshot, world.state());
}

#[test]
fn cloned_states_resume_identically() {
    let mut world = World::new(11);
    for _ in 0..25 {
        world.step();
    }
    let mut a = World::from_state(world.state().clone());
    let mut b = World::from_state(world.state().clone());
    for _ in 0..100 {
        a.step();
        b.step();
    }
    assert_eq!(a.state(), b.state());
    assert_eq!(a.tick(), 125);
}

#[test]
fn moving_rover_advances_deterministically() {
    let mut a = World::new(5);
    let mut b = World::new(5);
    let id = a.rovers().next().expect("rover exists").id;
    let start = a.rover(id).expect("rover exists").position;
    for world in [&mut a, &mut b] {
        let rover = world.rover_mut(id).expect("rover exists");
        rover.heading = FRAC_PI_2; // +Y
        rover.speed = 2.0; // world units per tick
    }
    assert_eq!(start, b.rover(id).expect("rover exists").position);
    for _ in 0..100 {
        a.step();
        b.step();
    }
    assert_eq!(a.state(), b.state());

    let position = a.rover(id).expect("rover exists").position;
    let delta = position - start;
    assert!(delta.x.abs() < 1e-9, "drift on X: {delta:?}");
    assert!(
        (delta.y - 200.0).abs() < 1e-9,
        "Y delta after 100 ticks: {delta:?}"
    );
}

#[test]
fn id_issuance_is_sequential_and_seed_independent() {
    let ids_a: Vec<_> = World::new(1).rovers().map(|r| r.id).collect();
    let ids_b: Vec<_> = World::new(987_654_321).rovers().map(|r| r.id).collect();
    assert_eq!(ids_a, ids_b);
    assert_eq!(ids_a[0], RoverId::from_raw(1));

    // A second spawn continues the same ascending sequence.
    let mut state = World::new(0).into_state();
    let second = state.spawn_rover(Vec2::ZERO, 0.0);
    assert_eq!(second, RoverId::from_raw(2));
    assert_eq!(state.rovers().count(), 2);

    // The issuer itself is deterministic.
    let mut issuer = IdIssuer::new();
    let first = issuer.issue();
    let next = issuer.issue();
    assert_eq!(first.raw() + 1, next.raw());
}
