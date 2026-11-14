//! Lua runtime for scene scripts, see `smelter_core::ScriptRuntime`.
//!
//! Scripts are written in Luau (https://luau.org), a dialect of Lua 5.1 designed to run untrusted
//! code. Every script runs in its own sandboxed VM:
//! - No access to files, network, environment or to code of other scripts.
//! - Memory used by the VM is limited.
//! - CPU time of every call into the script is limited. The limit is enforced inside loops,
//!   function calls and pattern matching. Library functions that can run for a long time without
//!   reaching any of those are wrapped to reject large arguments, see `prelude.luau`.
//! - Size of a scene returned by a script is limited, because it is converted by code that the
//!   VM can't interrupt.
//! - Only source code is loaded, never bytecode.

use std::time::Duration;

use smelter_core::{Script, ScriptError, ScriptOutputCtx, ScriptRuntime, ScriptSpec};

use crate::script::LuaScript;

mod budget;
mod convert;
mod script;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
pub struct LuaRuntimeOptions {
    /// Limit of memory used by a single script, in bytes.
    pub memory_limit: usize,
    /// Limit of CPU time for starting a script, which includes running its top-level code and
    /// rendering the first scene. The script is rejected when the limit is exceeded.
    pub start_time_limit: Duration,
    /// Limit of CPU time a script can use before a single frame. The script is terminated
    /// when the limit is exceeded, the output keeps rendering the last scene.
    pub update_time_limit: Duration,
}

impl Default for LuaRuntimeOptions {
    fn default() -> Self {
        Self {
            memory_limit: 16 * 1024 * 1024,
            start_time_limit: Duration::from_millis(100),
            update_time_limit: Duration::from_millis(10),
        }
    }
}

#[derive(Debug, Default)]
pub struct LuaScriptRuntime {
    options: LuaRuntimeOptions,
}

impl LuaScriptRuntime {
    pub fn new(options: LuaRuntimeOptions) -> Self {
        Self { options }
    }
}

impl ScriptRuntime for LuaScriptRuntime {
    fn load(
        &self,
        spec: &ScriptSpec,
        ctx: ScriptOutputCtx,
    ) -> Result<Box<dyn Script>, ScriptError> {
        Ok(Box::new(LuaScript::new(spec, ctx, self.options)?))
    }
}
