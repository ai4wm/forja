#[cfg(feature = "tui")]
mod viewer;

use forja_core::error::{ForjaError, Result};
use std::path::Path;

#[cfg(feature = "tui")]
pub fn launch_tui(audit_db: &Path, memory_db: &Path) -> Result<String> {
    let current_exe =
        std::env::current_exe().map_err(|error| ForjaError::Internal(error.to_string()))?;
    Command::new(current_exe)
        .arg("--tui-view")
        .arg(audit_db)
        .arg(memory_db)
        .spawn()
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    Ok("TUI viewer launched.".to_string())
}

#[cfg(not(feature = "tui"))]
pub fn launch_tui(_audit_db: &Path, _memory_db: &Path) -> Result<String> {
    Err(ForjaError::Internal(
        "TUI feature is not enabled in this build.".to_string(),
    ))
}

#[cfg(feature = "tui")]
pub fn maybe_run_tui_view(args: &[String]) -> Result<bool> {
    if args.len() != 4 || args[1] != "--tui-view" {
        return Ok(false);
    }

    viewer::run_tui_view(Path::new(&args[2]), Path::new(&args[3]))?;
    Ok(true)
}

#[cfg(not(feature = "tui"))]
pub fn maybe_run_tui_view(_args: &[String]) -> Result<bool> {
    Ok(false)
}
