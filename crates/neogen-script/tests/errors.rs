//! Backlog 2.5 — script error handling and lifecycle.

use neogen_core::{RoverId, Vec2, World};
use neogen_script::{
    LogEntry, MAX_LOG_BUFFER, RuntimeConfig, ScriptContext as _, ScriptError, ScriptHost,
    ScriptState, WorldContext,
};

fn host(seed: u64) -> ScriptHost {
    ScriptHost::new(World::new(seed), RuntimeConfig::default()).expect("host creates")
}

fn tight_host(seed: u64) -> ScriptHost {
    ScriptHost::new(
        World::new(seed),
        RuntimeConfig {
            instructions_per_tick: 4_000,
            hook_interval: 512,
            ..RuntimeConfig::default()
        },
    )
    .expect("host creates")
}

fn sole_rover(host: &ScriptHost) -> RoverId {
    host.rover_ids()[0]
}

#[test]
fn compile_error_at_create_leaves_the_world_intact() {
    let mut host = host(42);
    let rover = sole_rover(&host);
    let hash_before = host.world_state_hash();

    match host.create_script(rover, "return +") {
        Err(ScriptError::Compile { message }) => assert!(!message.is_empty()),
        other => panic!("expected Compile, got {other:?}"),
    }
    // Same for the managed path: nothing registers, nothing changes.
    match host.attach_script(rover, "if then") {
        Err(ScriptError::Compile { .. }) => {}
        other => panic!("expected Compile, got {other:?}"),
    }
    assert!(host.managed_ids().is_empty());
    assert_eq!(host.world_state_hash(), hash_before);
    host.step_world();
    assert_eq!(host.world_tick(), 1);
}

#[test]
fn runtime_error_fails_only_that_script() {
    let mut host = host(7);
    let rover = sole_rover(&host);

    let boom = host
        .attach_script(rover, "error('boom')")
        .expect("A attaches");
    let worker = host
        .attach_script(rover, "print(\"worker ok\") move(3, 4)")
        .expect("B attaches");
    assert_eq!(host.managed_ids(), vec![boom, worker]);

    for _ in 0..30 {
        host.step_world();
    }

    // A failed; the error is in the log with tick, id and text.
    assert!(matches!(
        host.script_state(boom),
        Some(ScriptState::Failed(ScriptError::Runtime { script_id, tick: 1, .. })) if script_id == boom
    ));
    let logs = host.log_snapshot();
    let boom_entry = logs
        .iter()
        .find(|e| e.script_id == boom && e.text.starts_with("error: "))
        .expect("boom error logged");
    assert_eq!(
        boom_entry.tick, 0,
        "failed on the first resume, world tick 0"
    );
    assert!(boom_entry.text.contains("boom"), "{}", boom_entry.text);

    // B finished its work; its line is logged; the rover moved.
    assert_eq!(host.script_state(worker), Some(ScriptState::Finished));
    assert!(
        logs.iter()
            .any(|e| e.script_id == worker && e.text == "worker ok"),
        "{logs:?}"
    );
    let position = host.rover_position(rover).expect("rover exists");
    assert!(
        position.distance_to(Vec2::new(3.0, 4.0)) < 1e-9,
        "{position:?}"
    );

    // The world kept ticking; the failed script stays failed.
    assert_eq!(host.world_tick(), 30);
    assert!(matches!(
        host.script_state(boom),
        Some(ScriptState::Failed(_))
    ));
}

#[test]
fn budget_suspension_is_not_a_failure() {
    let mut host = tight_host(11);
    let rover = sole_rover(&host);
    let spinner = host
        .attach_script(rover, "local x = 0 while true do x = x + 1 end")
        .expect("attaches");

    for _ in 0..10 {
        host.step_world();
        assert_eq!(
            host.script_state(spinner),
            Some(ScriptState::SuspendedBudget)
        );
    }
    // The world ticks, and nothing error-shaped was logged.
    assert_eq!(host.world_tick(), 10);
    let logs = host.log_snapshot();
    assert!(
        logs.iter().all(|e| !e.text.starts_with("error: ")),
        "{logs:?}"
    );
    assert!(host.script_state(spinner).unwrap().is_alive());
}

#[test]
fn restart_returns_a_failed_script_to_duty() {
    let mut host = host(13);
    let rover = sole_rover(&host);
    let id = host
        .attach_script(rover, "error('first life')")
        .expect("attaches");

    host.step_world();
    assert!(matches!(
        host.script_state(id),
        Some(ScriptState::Failed(_))
    ));

    host.restart_script(id, "print(\"reborn\") move(1, 1)")
        .expect("restart compiles");
    assert_eq!(host.script_state(id), Some(ScriptState::Running));

    for _ in 0..30 {
        host.step_world();
    }
    assert_eq!(host.script_state(id), Some(ScriptState::Finished));
    let logs = host.log_snapshot();
    assert!(
        logs.iter().any(|e| e.script_id == id && e.text == "reborn"),
        "{logs:?}"
    );
    let position = host.rover_position(rover).expect("rover exists");
    assert!(
        position.distance_to(Vec2::new(1.0, 1.0)) < 1e-9,
        "{position:?}"
    );
    // The same id was kept (identity survives the restart).
    assert_eq!(host.managed_ids(), vec![id]);
}

#[test]
fn restart_with_bad_source_keeps_the_old_state() {
    let mut host = host(17);
    let rover = sole_rover(&host);
    let id = host
        .attach_script(rover, "error('dead')")
        .expect("attaches");
    host.step_world();
    assert!(matches!(
        host.script_state(id),
        Some(ScriptState::Failed(_))
    ));

    match host.restart_script(id, "return +") {
        Err(ScriptError::Compile { .. }) => {}
        other => panic!("expected Compile, got {other:?}"),
    }
    // Still the old failure, still restartable.
    assert!(matches!(
        host.script_state(id),
        Some(ScriptState::Failed(_))
    ));
    host.restart_script(id, "print('second try')")
        .expect("restarts");
    assert_eq!(host.script_state(id), Some(ScriptState::Running));
}

#[test]
fn restart_does_not_leak_environments() {
    let mut host = host(19);
    let rover = sole_rover(&host);

    let noisy = host
        .attach_script(rover, "shared_secret = 42 print('noisy done')")
        .expect("attaches");
    let quiet = host
        .attach_script(rover, "return shared_secret == nil")
        .expect("attaches");
    host.step_world();
    assert_eq!(host.script_state(quiet), Some(ScriptState::Finished));

    // Restart the noisy script with a fresh chunk: its new env must be
    // empty of anything its previous life wrote.
    host.restart_script(noisy, "return shared_secret == nil")
        .expect("restarts");
    host.step_world();
    assert_eq!(host.script_state(noisy), Some(ScriptState::Finished));
}

#[test]
fn log_buffer_is_bounded_even_under_error_storm() {
    let mut host = tight_host(23);
    let rover = sole_rover(&host);
    // A script that fails, gets restarted, fails again — many times.
    let id = host
        .attach_script(rover, "error('storm')")
        .expect("attaches");
    for _ in 0..(MAX_LOG_BUFFER + 8) {
        host.step_world();
        if !host.script_state(id).unwrap().is_alive() {
            host.restart_script(id, "error('storm')").expect("restarts");
        }
    }
    let logs: Vec<LogEntry> = host.log_snapshot();
    assert!(logs.len() <= MAX_LOG_BUFFER, "{} entries", logs.len());
    assert!(logs.iter().all(|e| e.text.contains("storm")), "{logs:?}");
}

#[test]
fn world_context_still_logs_plain_entries() {
    // Bridge-side sanity: plain print entries keep flowing next to errors.
    let world = World::new(29);
    let mut context = WorldContext::new(world);
    let rover = context.world().rovers().next().expect("rover").id();
    context.log(LogEntry {
        tick: context.current_tick(),
        script_id: 1,
        rover_id: rover,
        text: "plain line".into(),
    });
    assert_eq!(context.logs().snapshot().len(), 1);
}

// ---- FIX-раунд фазы 2 B: #5 command cap, #9 stop/detach/getters ----

#[test]
fn command_spam_hits_the_per_tick_cap() {
    let mut host = host(37);
    let rover = sole_rover(&host);
    let id = host
        .attach_script(rover, "while true do move(0, 0) end")
        .expect("attaches");
    host.step_world();

    match host.script_state(id) {
        Some(ScriptState::Failed(ScriptError::Runtime { message, .. })) => {
            assert!(message.contains("command overflow"), "{message}");
        }
        other => panic!("expected Failed with overflow, got {other:?}"),
    }
    // The queue grew by at most the cap before the script died.
    assert!(
        host.rover_queue_len(rover).unwrap() <= neogen_script::MAX_COMMANDS_PER_TICK,
        "queue grew unbounded"
    );
    // The world keeps ticking.
    for _ in 0..5 {
        host.step_world();
    }
    assert_eq!(host.world_tick(), 6);
}

#[test]
fn stop_parks_a_script_until_restart() {
    let mut host = tight_host(41);
    let rover = sole_rover(&host);
    let id = host
        .attach_script(rover, "local x = 0 while true do x = x + 1 end")
        .expect("attaches");
    host.step_world();
    // The pure spinner suspends on budget — alive, no commands queued.
    assert_eq!(host.script_state(id), Some(ScriptState::SuspendedBudget));
    assert!(host.stop_script(id));
    assert_eq!(host.script_state(id), Some(ScriptState::Stopped));

    // Stopped scripts are never resumed: tick on, state unchanged.
    let queue_len = host.rover_queue_len(rover).unwrap();
    for _ in 0..10 {
        host.step_world();
    }
    assert_eq!(host.script_state(id), Some(ScriptState::Stopped));
    assert!(host.rover_queue_len(rover).unwrap() <= queue_len);

    // Revive via restart with fresh source.
    host.restart_script(id, "print('revived')")
        .expect("restarts");
    assert_eq!(host.script_state(id), Some(ScriptState::Running));
    host.step_world();
    assert_eq!(host.script_state(id), Some(ScriptState::Finished));

    // Stopping dead/unknown scripts is a no-op (false).
    assert!(!host.stop_script(id), "Finished script cannot be stopped");
    assert!(!host.stop_script(999));
}

#[test]
fn detach_removes_the_script_entirely() {
    let mut host = host(43);
    let rover = sole_rover(&host);
    let keeper = host
        .attach_script(rover, "print('keeper') move(2, 2)")
        .expect("attaches");
    let doomed = host
        .attach_script(rover, "while true do coroutine.yield() end")
        .expect("attaches");
    assert!(host.detach_script(doomed));
    assert_eq!(host.script_state(doomed), None);
    assert!(host.script_source(doomed).is_none());
    assert!(!host.detach_script(doomed), "already detached");

    // The survivor is unaffected and the world ticks on.
    for _ in 0..30 {
        host.step_world();
    }
    assert_eq!(host.script_state(keeper), Some(ScriptState::Finished));
    assert_eq!(host.managed_ids(), vec![keeper]);
}

#[test]
fn script_source_and_rover_getters() {
    let mut host = host(47);
    let rover = sole_rover(&host);
    let source = "print('identity')";
    let id = host.attach_script(rover, source).expect("attaches");
    assert_eq!(host.script_source(id), Some(source));
    assert_eq!(host.script_rover(id), Some(rover));
    assert_eq!(host.script_source(999), None);
    assert_eq!(host.script_rover(999), None);
}
