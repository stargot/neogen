//! Sandbox for player scripts (backlog 2.2).
//!
//! # Policy
//!
//! The environment is built by **whitelist**, not by blacklisting single
//! functions: at construction only `base`, `coroutine`, `table`, `string`,
//! `utf8` and `math` are ever loaded — `os`, `io`, `package` and `debug`
//! never enter the state. From the base library the dangerous standalone
//! globals (`load`, `loadfile`, `dofile`, `require`, `collectgarbage`) are
//! removed right after creation, defensively together with the never-loaded
//! tables themselves (belt and braces against mlua changing its default
//! safe subset).
//!
//! `collectgarbage` is **removed entirely**, not stubbed as a no-op: player
//! scripts must not control or observe the garbage collector — collection
//! cadence is owned by the host (Rust), and a lying no-op would suggest a
//! capability that does not exist.
//!
//! # Init-order invariant
//!
//! The sandbox is installed on a freshly created Lua state **before any
//! player code executes**. Nothing that runs later can hold a saved
//! reference to the removed globals, because they never were reachable:
//! `_G` points at the same stripped globals table, and in Lua 5.4 the old
//! `getfenv`/`setfenv` escape hatches are gone — `_ENV` tricks only rebind
//! a chunk to (a copy of) the same stripped table. With `load` absent,
//! scripts cannot compile fresh chunks that captured a different env.
//!
//! The built-in `print` is removed: script environments receive a
//! LogBuffer-backed replacement installed per script (see `api`).

use mlua::{Lua, StdLib};

/// Standalone globals removed from the environment (all of `os`/`io`/
/// `package`/`debug` are listed defensively even though the whitelist
/// never loads them).
const REMOVED_GLOBALS: &[&str] = &[
    "os",
    "io",
    "package",
    "debug",
    "load",
    "loadfile",
    "dofile",
    "require",
    "collectgarbage",
    // 2.4: the built-in print is removed; script environments get a
    // LogBuffer-backed replacement (see `api`).
    "print",
];

/// The safe standard library subset: coroutines for the future scheduler
/// plus the pure-data libraries. The `base` library has no flag in mlua —
/// it is always loaded, which is exactly why the standalone dangerous
/// globals from it are removed explicitly (see [`REMOVED_GLOBALS`]).
pub(crate) fn safe_libs() -> StdLib {
    StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH
}

/// Install the sandbox on a fresh Lua state.
///
/// Must be called exactly once, immediately after state creation, before
/// any player code runs (see the init-order invariant in the module docs).
pub(crate) fn install(lua: &Lua) -> Result<(), crate::ScriptError> {
    let globals = lua.globals();
    for name in REMOVED_GLOBALS {
        // raw: bypass any metatable; removing an absent key is a no-op.
        globals
            .raw_remove(*name)
            .map_err(|error| crate::ScriptError::runtime(0, 0, &error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_libs_exclude_os_io_package_debug() {
        let libs = safe_libs();
        for kept in [
            StdLib::COROUTINE,
            StdLib::TABLE,
            StdLib::STRING,
            StdLib::UTF8,
            StdLib::MATH,
        ] {
            assert!(libs.contains(kept));
        }
        for banned in [StdLib::OS, StdLib::IO, StdLib::PACKAGE, StdLib::DEBUG] {
            assert!(!libs.contains(banned));
        }
    }
}
