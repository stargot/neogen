//! Backlog 2.2 — sandboxed environment: no os/io/load-family, runtime
//! survives script errors.

use neogen_script::{EvalValue, Runtime};

fn fresh() -> Runtime {
    Runtime::new().expect("runtime creates")
}

#[test]
fn os_execute_is_a_script_error_not_an_execution() {
    let runtime = fresh();
    // Must fail at "attempt to index a nil value (global 'os')" — the
    // command must never reach the shell.
    let err = runtime
        .eval("return os.execute('echo pwned')")
        .expect_err("os is not available");
    let msg = err.to_string();
    assert!(msg.contains("os"), "unexpected error: {msg}");
}

#[test]
fn io_open_is_a_script_error() {
    let runtime = fresh();
    assert!(runtime.eval("return io.open('x.txt')").is_err());
}

#[test]
fn load_family_is_a_script_error() {
    let runtime = fresh();
    assert!(runtime.eval("return load('return 1')").is_err());
    assert!(runtime.eval("return loadfile('any.lua')").is_err());
    assert!(runtime.eval("return dofile('any.lua')").is_err());
}

#[test]
fn require_is_a_script_error() {
    let runtime = fresh();
    assert!(runtime.eval("return require('string')").is_err());
}

#[test]
fn collectgarbage_is_a_script_error() {
    let runtime = fresh();
    assert!(runtime.eval("return collectgarbage('count')").is_err());
}

#[test]
fn runtime_survives_every_sandbox_error() {
    let runtime = fresh();
    for hostile in [
        "return os.execute('echo hi')",
        "return io.open('x')",
        "return load('return 1')",
        "return require('os')",
        "return collectgarbage()",
        "error('deliberate')",
    ] {
        assert!(runtime.eval(hostile).is_err(), "expected error: {hostile}");
        // The same runtime keeps working after each failure.
        assert_eq!(
            runtime.eval("return 1 + 1").expect("runtime still alive"),
            EvalValue::Number(2.0),
            "broken after {hostile}"
        );
    }
}

#[test]
fn print_is_still_available() {
    // Built-in print stays until 2.4 swaps it for a LogBuffer-backed one
    // (see sandbox.rs docs).
    let runtime = fresh();
    let value = runtime
        .eval("print('sandbox smoke') return 42")
        .expect("print works");
    assert_eq!(value, EvalValue::Number(42.0));
}

#[test]
fn sandboxed_globals_visibility() {
    let runtime = fresh();
    // Kept: the whitelist.
    for name in [
        "math",
        "string",
        "table",
        "coroutine",
        "utf8",
        "print",
        "pcall",
    ] {
        assert!(runtime.has_global(name), "{name} should be visible");
    }
    // Removed.
    for name in [
        "os",
        "io",
        "package",
        "debug",
        "load",
        "loadfile",
        "dofile",
        "require",
        "collectgarbage",
    ] {
        assert!(!runtime.has_global(name), "{name} must be invisible");
    }
}

#[test]
fn the_globals_table_itself_is_stripped() {
    // _G points at the same stripped table — nil, not an error, and no
    // back door through raw access.
    let runtime = fresh();
    assert_eq!(
        runtime.eval("return _G.os").expect("read as nil"),
        EvalValue::Nil
    );
    assert_eq!(
        runtime
            .eval("return rawget(_G, 'load')")
            .expect("read as nil"),
        EvalValue::Nil
    );
    assert_eq!(
        runtime.eval("return type(_G.require)").expect("evals"),
        EvalValue::String("nil".to_string())
    );
}
