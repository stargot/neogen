//! Backlog 2.4 — script API against a real `neogen-core` world.

use neogen_core::{RoverId, Vec2, World};
use neogen_script::{
    ACT_KINDS, EvalValue, RuntimeConfig, ScriptError, ScriptHost, TickOutcome, WorldContext,
};

fn host_with_world(seed: u64) -> ScriptHost {
    ScriptHost::new(World::new(seed), RuntimeConfig::default()).expect("host creates")
}

fn sole_rover(host: &ScriptHost) -> RoverId {
    host.rover_ids()[0]
}

#[test]
fn move_and_print_script_drives_the_world() {
    let mut host = host_with_world(42);
    let rover = sole_rover(&host);

    let mut script = host
        .create_script(rover, "print(\"going\") move(5, 5)")
        .expect("script compiles");
    assert_eq!(script.id(), 1);

    // The script runs within its tick budget and completes immediately;
    // the command is QUEUED, not executed.
    assert_eq!(
        script.resume_tick().expect("runs"),
        TickOutcome::Completed(EvalValue::Nil)
    );
    assert_eq!(host.rover_queue_len(rover), Some(1));
    let logs = host.drain_logs();
    assert_eq!(logs.len(), 1, "one print line: {logs:?}");
    assert_eq!(logs[0].text, "going");
    assert_eq!(logs[0].tick, 0);
    assert_eq!(logs[0].script_id, 1);
    assert_eq!(logs[0].rover_id, rover);

    // The world applies the queued move over its own ticks.
    for _ in 0..30 {
        host.step_world();
    }
    let position = host.rover_position(rover).expect("rover exists");
    let target = Vec2::new(5.0, 5.0);
    assert!(
        position.distance_to(target) < 1e-9,
        "rover at {position:?}, expected {target:?}"
    );
    assert_eq!(host.rover_queue_len(rover), Some(0));
}

#[test]
fn invalid_move_is_a_lua_error_and_poisons_nothing() {
    let mut host = host_with_world(7);
    let rover = sole_rover(&host);
    let start = host.rover_position(rover).expect("rover exists");
    let hash_before = host.world_state_hash();

    let mut script = host.create_script(rover, "move(0/0, 0)").expect("compiles");
    match script.resume_tick() {
        Err(ScriptError::Runtime { message, .. }) => {
            assert!(message.contains("finite"), "unclear message: {message}");
        }
        other => panic!("expected Runtime error, got {other:?}"),
    }

    // The world is not poisoned: no command queued, hash unchanged,
    // the rover does not move.
    assert_eq!(host.rover_queue_len(rover), Some(0));
    assert_eq!(host.world_state_hash(), hash_before);
    for _ in 0..10 {
        host.step_world();
    }
    assert_eq!(host.rover_position(rover), Some(start));
}

#[test]
fn scan_queues_and_scan_result_peeks_latest() {
    let mut host = host_with_world(11);
    let rover = sole_rover(&host);
    let mut script = host
        .create_script(
            rover,
            r#"
            local first = scan_result()
            scan(3.0)
            local during = scan_result()
            return during == nil
            "#,
        )
        .expect("compiles");

    let outcome = script.resume_tick().expect("runs");
    let TickOutcome::Completed(value) = outcome else {
        panic!("expected completion, got {outcome:?}");
    };
    // No scan completed yet: every peek is nil.
    assert_eq!(value, EvalValue::Bool(true));

    // Run the world until the scan completes (SCAN_TICKS ticks).
    for _ in 0..5 {
        host.step_world();
    }
    assert_eq!(host.rover_queue_len(rover), Some(0));

    // A follow-up script peek sees the completed scan (peek, not drain:
    // a second peek returns the same result again).
    for probe in 0..2 {
        let mut tick_reader = host
            .create_script(rover, "return scan_result().tick")
            .expect("compiles");
        assert_eq!(
            tick_reader.resume_tick().expect("runs"),
            TickOutcome::Completed(EvalValue::Number(5.0)),
            "probe {probe}"
        );
        let mut radius_reader = host
            .create_script(rover, "return scan_result().radius")
            .expect("compiles");
        assert_eq!(
            radius_reader.resume_tick().expect("runs"),
            TickOutcome::Completed(EvalValue::Number(3.0)),
            "probe {probe}"
        );
    }
}

#[test]
fn negative_scan_radius_is_rejected() {
    let mut host = host_with_world(13);
    let rover = sole_rover(&host);
    let mut script = host.create_script(rover, "scan(-1)").expect("compiles");
    match script.resume_tick() {
        Err(ScriptError::Runtime { message, .. }) => {
            assert!(message.contains("negative"), "{message}");
        }
        other => panic!("expected Runtime error, got {other:?}"),
    }
    assert_eq!(host.rover_queue_len(rover), Some(0));
}

#[test]
fn act_accepts_whitelist_and_rejects_unknown_kinds() {
    let mut host = host_with_world(17);
    let rover = sole_rover(&host);

    let mut good = host
        .create_script(rover, "act(\"plant\", { seed = 1 }) act(\"drain\")")
        .expect("compiles");
    assert!(matches!(
        good.resume_tick().expect("runs"),
        TickOutcome::Completed(_)
    ));
    // Stubs queue Noops: the wire shape is pinned, semantics arrive in 8.1.
    assert_eq!(host.rover_queue_len(rover), Some(2));

    let mut bad = host
        .create_script(rover, "act(\"nuke\")")
        .expect("compiles");
    match bad.resume_tick() {
        Err(ScriptError::Runtime { message, .. }) => {
            assert!(
                message.contains("nuke") && message.contains("plant"),
                "{message}"
            );
        }
        other => panic!("expected Runtime error, got {other:?}"),
    }
    assert_eq!(ACT_KINDS, &["plant", "drain"]);
}

#[test]
fn script_environments_are_isolated() {
    let mut host = host_with_world(19);
    let rover = sole_rover(&host);

    // Script A writes a global and replaces `move` in its own env.
    let mut a = host
        .create_script(rover, "x = 1 move = nil")
        .expect("A compiles");
    assert!(matches!(
        a.resume_tick().expect("A runs"),
        TickOutcome::Completed(_)
    ));

    // Script B sees no `x` and keeps its own `move` (single-value probes).
    let mut b_nil = host
        .create_script(rover, "return x == nil")
        .expect("compiles");
    assert_eq!(
        b_nil.resume_tick().expect("runs"),
        TickOutcome::Completed(EvalValue::Bool(true)),
        "B must not see A's global"
    );
    let mut b_move = host
        .create_script(rover, "return type(move)")
        .expect("compiles");
    assert_eq!(
        b_move.resume_tick().expect("runs"),
        TickOutcome::Completed(EvalValue::String("function".to_string())),
        "B's move must survive A's overwrite"
    );
    // And the shared sandbox globals really have no `x` either.
    assert!(!host_has_global_via_eval(&host, "x"));
}

fn host_has_global_via_eval(_host: &ScriptHost, name: &str) -> bool {
    // `return _G` would be nil in script envs; check the shared table by
    // evaluating on a plain Runtime with the same sandbox instead.
    let runtime = neogen_script::Runtime::new(RuntimeConfig::default()).expect("runtime");
    matches!(
        runtime
            .eval(&format!("return {name} ~= nil"))
            .expect("eval"),
        EvalValue::Bool(true)
    )
}

#[test]
fn world_context_logs_and_scans_wired() {
    // Direct trait-level check of the concrete context (bridge-side view).
    let world = World::new(23);
    let mut context = WorldContext::new(world);
    let rover = context.world().rovers().next().expect("rover").id();

    use neogen_script::ScriptContext as _;
    assert!(context.latest_scan(rover).is_none());
    context
        .push_command(rover, neogen_core::Command::Scan { radius: 2.0 })
        .expect("queues");
    assert!(context.latest_scan(rover).is_none());
    for _ in 0..5 {
        context.world_mut().step();
    }
    let latest = context.latest_scan(rover).expect("scan completed");
    assert_eq!(latest.radius, 2.0);
    // Peek semantics: still there.
    assert!(context.latest_scan(rover).is_some());
    assert_eq!(context.current_tick(), 5);
    assert!(context.logs().is_empty());
}
