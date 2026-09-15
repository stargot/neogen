//! Backlog 2.2 — sandboxed environment: no os/io/load-family, runtime
//! survives script errors.

use neogen_script::{EvalValue, Runtime, RuntimeConfig};

fn fresh() -> Runtime {
    Runtime::new(RuntimeConfig::default()).expect("runtime creates")
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
fn builtin_print_is_gone_from_globals() {
    // Since 2.4 the built-in print is removed from the shared globals;
    // scripts get a LogBuffer-backed replacement in their private env
    // (see ScriptHost::create_script and api::install).
    let runtime = fresh();
    assert!(!runtime.has_global("print"));
    assert!(runtime.eval("print('raw globals must not print')").is_err());
    // Sandbox globals keep working for non-print code.
    assert_eq!(
        runtime.eval("return 42").expect("eval works"),
        EvalValue::Number(42.0)
    );
}

#[test]
fn sandboxed_globals_visibility() {
    let runtime = fresh();
    // Kept: the whitelist.
    for name in ["math", "string", "table", "coroutine", "utf8", "pcall"] {
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
        "print",
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
