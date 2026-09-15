//! Backlog 2.3 — per-tick instruction budget.

use neogen_script::{
    DEFAULT_INSTRUCTIONS_PER_TICK, EvalValue, Runtime, RuntimeConfig, ScriptError, TickOutcome,
};

fn tight_runtime() -> Runtime {
    Runtime::new(RuntimeConfig {
        instructions_per_tick: 4_000, // ~8 intervals of 512
        hook_interval: 512,
        ..RuntimeConfig::default()
    })
    .expect("runtime creates")
}

#[test]
fn infinite_loop_hits_budget_not_a_hang() {
    let mut runtime = tight_runtime();
    let mut script = runtime
        .create_script("while true do end")
        .expect("compiles");
    match script.resume_tick() {
        Err(ScriptError::BudgetExceeded {
            script_id,
            tick,
            consumed,
        }) => {
            assert_eq!(script_id, 1);
            assert_eq!(tick, 1);
            assert!(consumed >= 4096, "consumed: {consumed}");
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
    // The coroutine is suspended, not dead.
    assert!(!script.is_finished());
}

#[test]
fn simulation_step_runs_after_exhaustion() {
    // A "simulation step" stub must run normally while the script sits
    // suspended — the genre contract of 2.3.
    let mut runtime = tight_runtime();
    let mut script = runtime
        .create_script("while true do end")
        .expect("compiles");
    assert!(script.resume_tick().is_err()); // BudgetExceeded

    let mut steps = 0;
    for _ in 0..10 {
        // world.step() equivalent — here just runtime bookkeeping + eval.
        assert_eq!(
            runtime.eval("return 1 + 1").expect("world still alive"),
            EvalValue::Number(2.0)
        );
        steps += 1;
    }
    assert_eq!(steps, 10);
    // And the suspended script reports its tick counter honestly.
    assert_eq!(script.tick(), 1);
}

#[test]
fn script_resumes_from_the_suspension_point() {
    let mut runtime = tight_runtime();
    // A loop too big for one tick: the global `hits` counter is the
    // observable side effect.
    let mut script = runtime
        .create_script("hits = 0 for i = 1, 100000 do hits = hits + 1 end")
        .expect("compiles");

    let mut ticks = 0;
    loop {
        ticks += 1;
        match script.resume_tick() {
            Err(ScriptError::BudgetExceeded { .. }) => {
                let hits = runtime
                    .eval("return hits")
                    .expect("globals readable between ticks");
                let EvalValue::Number(hits) = hits else {
                    panic!("hits is not a number");
                };
                // Progress was made and the coroutine is alive mid-loop.
                assert!(hits > 0.0 && hits < 100_000.0, "mid-run hits: {hits}");
                assert!(!script.is_finished());
                assert!(ticks < 100, "loop never finishes");
            }
            Ok(TickOutcome::Completed(_)) => break,
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
    // It did finish, with the exact side effect the script intended —
    // resumption never re-ran or skipped iterations.
    assert_eq!(
        runtime.eval("return hits").expect("final hits"),
        EvalValue::Number(100_000.0)
    );
    assert!(ticks > 1, "should have taken several ticks");
}

#[test]
fn budget_exhaustion_is_deterministic_not_timed() {
    // The same script hits the budget at the same instruction counts on
    // every run (and across separately created runtimes).
    for _ in 0..3 {
        let mut runtime = tight_runtime();
        let mut a = runtime.create_script(SPIN).expect("compiles");
        let mut b = runtime.create_script(SPIN).expect("compiles");
        let a_consumed = match a.resume_tick() {
            Err(ScriptError::BudgetExceeded { consumed, .. }) => consumed,
            other => panic!("expected BudgetExceeded, got {other:?}"),
        };
        let b_consumed = match b.resume_tick() {
            Err(ScriptError::BudgetExceeded { consumed, .. }) => consumed,
            other => panic!("expected BudgetExceeded, got {other:?}"),
        };
        assert_eq!(a_consumed, b_consumed);
        // A second tick on the same script consumes the same allowance.
        let again = match a.resume_tick() {
            Err(ScriptError::BudgetExceeded { consumed, .. }) => consumed,
            other => panic!("expected BudgetExceeded, got {other:?}"),
        };
        assert_eq!(a_consumed, again);
    }
}

#[test]
fn normal_script_completes_within_budget() {
    let mut runtime = Runtime::new(RuntimeConfig::default()).expect("runtime creates");
    let mut script = runtime
        .create_script("local sum = 0 for i = 1, 1000 do sum = sum + i end return sum")
        .expect("compiles");
    let outcome = script.resume_tick().expect("fits the default budget");
    assert_eq!(
        outcome,
        TickOutcome::Completed(EvalValue::Number(500_500.0))
    );
    assert!(script.is_finished());
}

#[test]
fn scripts_own_yield_is_not_budget_exceeded() {
    let mut runtime = tight_runtime();
    let mut script = runtime
        .create_script("coroutine.yield(7) return 8")
        .expect("compiles");
    assert_eq!(
        script.resume_tick().expect("own yield"),
        TickOutcome::Yielded(EvalValue::Number(7.0))
    );
    assert!(!script.is_finished());
    assert_eq!(
        script.resume_tick().expect("continues"),
        TickOutcome::Completed(EvalValue::Number(8.0))
    );
}

#[test]
fn script_runtime_error_is_reported() {
    let mut runtime = tight_runtime();
    let mut script = runtime
        .create_script("error('boom on purpose')")
        .expect("compiles");
    match script.resume_tick() {
        Err(ScriptError::Runtime { message, .. }) => {
            assert!(message.contains("boom"), "{message}")
        }
        other => panic!("expected Runtime error, got {other:?}"),
    }
    assert!(script.is_finished());
}

#[test]
fn syntax_error_surfaces_at_spawn() {
    let mut runtime = tight_runtime();
    assert!(runtime.create_script("return +").is_err());
}

#[test]
fn default_budget_matches_backlog() {
    assert_eq!(DEFAULT_INSTRUCTIONS_PER_TICK, 100_000);
}

/// Deterministic spinner used by the determinism test.
const SPIN: &str = "local x = 0 while true do x = x + 1 end";

// ---- FIX-раунд фазы 2: #3 memory limit ----

#[test]
fn giant_c_function_hits_memory_limit_not_oom() {
    // string.rep is a C function: no instruction hook inside it, but the
    // allocator limit stops it. 512 MiB request against the 32 MiB cap.
    let mut runtime = Runtime::new(RuntimeConfig::default()).expect("runtime");
    let mut script = runtime
        .create_script("return string.rep('x', 512 * 1024 * 1024)")
        .expect("compiles");
    match script.resume_tick() {
        Err(ScriptError::Runtime { message, .. }) => {
            assert!(
                message.to_lowercase().contains("memory"),
                "expected a memory error, got: {message}"
            );
        }
        other => panic!("expected Runtime error, got {other:?}"),
    }
    assert!(script.is_finished());
}

#[test]
fn small_allocations_still_work() {
    let mut runtime = Runtime::new(RuntimeConfig::default()).expect("runtime");
    let mut script = runtime
        .create_script("return string.rep('x', 1024)")
        .expect("compiles");
    assert_eq!(
        script.resume_tick().expect("runs"),
        TickOutcome::Completed(EvalValue::String("x".repeat(1024)))
    );
}
