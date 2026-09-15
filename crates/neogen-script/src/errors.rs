//! Script error taxonomy — the MVP-final form (backlog 2.5).
//!
//! Every variant carries enough context for the console (message + tick of
//! occurrence); `Display` never panics and always produces readable text.

use core::fmt;

/// Hard cap on error text carried into [`ScriptError`] variants and the
/// log buffer — a pathological traceback must not flood memory.
pub const MAX_ERROR_TEXT: usize = 512;

/// Failure inside the script runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptError {
    /// The source failed to compile (syntax error) — surfaced at script
    /// creation/restart, before any tick ran.
    Compile {
        /// Compiler message (already truncated to [`MAX_ERROR_TEXT`]).
        message: String,
    },
    /// The script raised a Lua runtime error. The message embeds the Lua
    /// traceback where mlua provides one (truncated).
    Runtime {
        /// Script the error belongs to (`0` marks host-side `eval`, which
        /// has no script identity — ids start at 1).
        script_id: u32,
        /// Tick the script was on when it failed.
        tick: u64,
        /// Error message with traceback, truncated.
        message: String,
    },
    /// The per-tick instruction allowance is spent. The coroutine is
    /// *suspended, not killed* — the next `resume_tick` continues from the
    /// same instruction. This is control flow (a pause), never a failure.
    BudgetExceeded {
        /// Id of the script.
        script_id: u32,
        /// Tick number of the exhausted resume.
        tick: u64,
        /// Instructions consumed this tick (multiple of the hook interval).
        consumed: u64,
    },
}

impl ScriptError {
    /// Build a [`ScriptError::Compile`] from an mlua error, truncating.
    pub(crate) fn compile(error: &mlua::Error) -> Self {
        Self::Compile {
            message: truncate(&error.to_string()),
        }
    }

    /// Build a [`ScriptError::Runtime`] from an mlua error, truncating.
    pub(crate) fn runtime(script_id: u32, tick: u64, error: &mlua::Error) -> Self {
        Self::Runtime {
            script_id,
            tick,
            message: truncate(&error.to_string()),
        }
    }

    /// Tick of occurrence (0 for compile — nothing ran yet).
    pub fn tick(&self) -> u64 {
        match self {
            Self::Compile { .. } => 0,
            Self::Runtime { tick, .. } | Self::BudgetExceeded { tick, .. } => *tick,
        }
    }
}

/// One-line text for the console, prefixed the way the host logs it.
pub(crate) fn log_line(error: &ScriptError) -> String {
    format!("error: {error}")
}

/// Truncate to [`MAX_ERROR_TEXT`] with an honest ellipsis marker.
fn truncate(text: &str) -> String {
    if text.len() <= MAX_ERROR_TEXT {
        return text.to_string();
    }
    // Cut on a char boundary near the cap.
    let mut end = MAX_ERROR_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated]", &text[..end])
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile { message } => write!(f, "script failed to compile: {message}"),
            Self::Runtime {
                script_id,
                tick,
                message,
            } => write!(f, "script {script_id} failed at tick {tick}: {message}"),
            Self::BudgetExceeded {
                script_id,
                tick,
                consumed,
            } => write!(
                f,
                "script {script_id} exhausted its budget at tick {tick} \
                 ({consumed} instructions consumed); suspended, not killed"
            ),
        }
    }
}

impl std::error::Error for ScriptError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_readable_for_every_variant() {
        let compile = ScriptError::Compile {
            message: "syntax error near '+'".into(),
        };
        assert!(compile.to_string().contains("compile"));
        assert!(compile.to_string().contains("'+'"));
        assert_eq!(compile.tick(), 0);

        let runtime = ScriptError::Runtime {
            script_id: 3,
            tick: 12,
            message: "boom\nstack traceback...".into(),
        };
        let text = runtime.to_string();
        assert!(
            text.contains('3') && text.contains("12") && text.contains("boom"),
            "{text}"
        );
        assert_eq!(runtime.tick(), 12);

        let budget = ScriptError::BudgetExceeded {
            script_id: 1,
            tick: 4,
            consumed: 4096,
        };
        assert!(budget.to_string().contains("suspended"));
        assert_eq!(budget.tick(), 4);
    }

    #[test]
    fn long_messages_are_truncated() {
        let long = "x".repeat(MAX_ERROR_TEXT + 100);
        let error = ScriptError::Runtime {
            script_id: 1,
            tick: 1,
            message: truncate(&long),
        };
        let ScriptError::Runtime { message, .. } = &error else {
            panic!("wrong variant");
        };
        assert!(message.len() < MAX_ERROR_TEXT + 32);
        assert!(message.ends_with("…[truncated]"));
    }

    #[test]
    fn log_line_is_prefixed() {
        let error = ScriptError::Compile {
            message: "oops".into(),
        };
        assert!(log_line(&error).starts_with("error: "));
    }
}
