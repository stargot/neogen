//! Backlog 2.1 — first Lua run through the runtime.

use neogen_script::{EvalValue, Runtime, RuntimeConfig};

#[test]
fn hello_lua_evals_two() {
    let runtime = Runtime::new(RuntimeConfig::default()).expect("runtime creates");
    let value = runtime.eval("return 1 + 1").expect("eval succeeds");
    assert_eq!(value, EvalValue::Number(2.0));
}

#[test]
fn the_result_is_a_number_not_a_string() {
    let runtime = Runtime::new(RuntimeConfig::default()).expect("runtime creates");
    let value = runtime.eval("return 1 + 1").expect("eval succeeds");
    assert!(matches!(value, EvalValue::Number(_)), "got {value:?}");
    let EvalValue::Number(n) = value else {
        panic!("expected a number, got {value:?}");
    };
    assert_eq!(n, 2.0);
    // A string result stays a string — the mapping is honest.
    let s = runtime
        .eval("return tostring(1 + 1)")
        .expect("eval succeeds");
    assert_eq!(s, EvalValue::String("2".to_string()));
}
