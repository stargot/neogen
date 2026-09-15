//! Backlog 1.5 — command-driven rover movement.

use neogen_core::{Command, DEFAULT_CRUISE_SPEED, RoverId, SCAN_TICKS, Vec2, World, state_hash};

fn sole_rover(world: &World) -> (RoverId, Vec2) {
    let rover = world.rovers().next().expect("rover exists");
    (rover.id, rover.position)
}

#[test]
fn move_to_arrives_exactly_without_oscillation() {
    let mut world = World::new(42);
    let (id, start) = sole_rover(&world);
    // Distance 50, cruise 3: ticks 1..=16 move 3 each (48 < 50), tick 17
    // covers the remaining 2 and lands exactly — a non-round tick count.
    world.rover_mut(id).expect("rover exists").speed = 3.0;
    let target = start + Vec2::new(30.0, 40.0); // |(30, 40)| = 50
    world.push_commands(id, [Command::MoveTo { target }]);

    for expected_tick in 1..=16u64 {
        world.step();
        let left = world
            .rover(id)
            .expect("rover exists")
            .position
            .distance_to(target);
        assert!(
            (left - (50.0 - 3.0 * expected_tick as f64)).abs() < 1e-9,
            "tick {expected_tick}: {left} left"
        );
    }

    world.step(); // tick 17: lands
    let rover = world.rover(id).expect("rover exists");
    assert!(
        rover.position.distance_to(target) < 1e-12,
        "not exactly on target: {:?}",
        rover.position
    );
    assert!(rover.commands.is_empty());

    // No oscillation or drift afterwards.
    for _ in 0..5 {
        world.step();
    }
    let rover = world.rover(id).expect("rover exists");
    assert_eq!(rover.position, target);
}

#[test]
fn commands_execute_in_order() {
    let mut world = World::new(7);
    let (id, start) = sole_rover(&world);
    // Default cruise (speed field 0 → DEFAULT_CRUISE_SPEED = 2):
    // A is 2 away → 1 tick; B is 4 more → 2 ticks; scan → SCAN_TICKS.
    let a = start + Vec2::new(2.0, 0.0);
    let b = a + Vec2::new(0.0, 4.0);
    world.push_commands(
        id,
        [
            Command::MoveTo { target: a },
            Command::MoveTo { target: b },
            Command::Scan { radius: 5.0 },
        ],
    );

    world.step(); // tick 1: at A, two commands left
    let rover = world.rover(id).expect("rover exists");
    assert_eq!(rover.position, a);
    assert_eq!(rover.commands.len(), 2);

    world.step();
    world.step(); // ticks 2-3: at B
    assert_eq!(world.rover(id).expect("rover exists").position, b);

    for _ in 0..SCAN_TICKS {
        world.step(); // ticks 4..=8: scan
    }
    let rover = world.rover(id).expect("rover exists");
    assert_eq!(rover.position, b);
    assert!(rover.commands.is_empty());
    assert_eq!(rover.scan_buffer.len(), 1);
}

#[test]
fn scan_takes_exactly_scan_ticks_and_buffers_result() {
    let mut world = World::new(11);
    let (id, _) = sole_rover(&world);
    world.push_commands(id, [Command::Scan { radius: 7.5 }]);

    for tick in 1..SCAN_TICKS {
        world.step();
        assert!(
            world
                .rover(id)
                .expect("rover exists")
                .scan_buffer
                .is_empty(),
            "scan finished early at tick {tick}"
        );
    }

    world.step(); // tick SCAN_TICKS: completes
    let rover = world.rover(id).expect("rover exists");
    assert!(rover.commands.is_empty());
    let result = &rover.scan_buffer[0];
    assert_eq!(result.tick, SCAN_TICKS);
    assert_eq!(result.radius, 7.5);
    assert!(result.points.is_empty()); // placeholder until phase 7
}

#[test]
fn noop_completes_in_one_tick() {
    let mut world = World::new(3);
    let (id, start) = sole_rover(&world);
    world.push_commands(id, [Command::Noop]);
    world.step();
    let rover = world.rover(id).expect("rover exists");
    assert!(rover.commands.is_empty());
    assert_eq!(rover.position, start);
}

#[test]
fn same_seed_same_commands_same_hash() {
    for seed in [42u64, 7] {
        let script = |target: Vec2| {
            [
                Command::MoveTo { target },
                Command::Scan { radius: 3.0 },
                Command::Noop,
                Command::MoveTo { target: Vec2::ZERO },
            ]
        };
        let mut a = World::new(seed);
        let mut b = World::new(seed);
        let (id_a, start_a) = sole_rover(&a);
        let (id_b, start_b) = sole_rover(&b);
        assert_eq!(start_a, start_b);
        a.push_commands(id_a, script(start_a + Vec2::new(5.0, 5.0)));
        b.push_commands(id_b, script(start_b + Vec2::new(5.0, 5.0)));

        for _ in 0..50 {
            a.step();
            b.step();
        }
        assert_eq!(
            state_hash(a.state()),
            state_hash(b.state()),
            "seed {seed}: hashes diverged"
        );
        assert_eq!(a.state(), b.state());
    }
}

#[test]
fn different_commands_produce_different_states() {
    let mut a = World::new(42);
    let mut b = World::new(42);
    let (id_a, start_a) = sole_rover(&a);
    let (id_b, start_b) = sole_rover(&b);
    assert_eq!(start_a, start_b);
    a.push_commands(
        id_a,
        [Command::MoveTo {
            target: start_a + Vec2::new(5.0, 0.0),
        }],
    );
    b.push_commands(
        id_b,
        [Command::MoveTo {
            target: start_b + Vec2::new(-5.0, 0.0),
        }],
    );
    for _ in 0..10 {
        a.step();
        b.step();
    }
    // Same physics shape but different positions → different hashes.
    assert_ne!(state_hash(a.state()), state_hash(b.state()));
}

#[test]
fn default_cruise_speed_is_sane() {
    // Used in the other tests via speed-field zeroing; assert through a
    // non-const expression so the check stays meaningful.
    let speed = DEFAULT_CRUISE_SPEED;
    assert!(speed > 0.0);
    let mut rover_world = World::new(1);
    let (id, start) = sole_rover(&rover_world);
    rover_world.push_commands(
        id,
        [Command::MoveTo {
            target: start + Vec2::new(speed, 0.0),
        }],
    );
    rover_world.step();
    assert_eq!(
        rover_world.rover(id).expect("rover exists").position.x,
        start.x + speed
    );
}
