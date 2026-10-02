//! Compile the production input adapter separately from the browser binary's
//! shared asset modules, whose test-only reference painters belong to wgpu.

use anyhow::{Result, anyhow};

trait ScriptResultExt<T> {
    fn browser(self) -> Result<T>;
}

impl<T> ScriptResultExt<T> for std::result::Result<T, stella_script::ScriptError> {
    fn browser(self) -> Result<T> {
        self.map_err(|error| anyhow!(error.to_string()))
    }
}

#[path = "../src/input.rs"]
mod input;
